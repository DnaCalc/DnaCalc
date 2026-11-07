//! Exact decimal arithmetic for the rounding and integer functions: how such a function reads a
//! number as an exact decimal, the integer parts of a binary64 found from its bits, and the
//! unbounded integers that rounding at a decimal position, multiples and remainders need. Every
//! step is exact integer arithmetic on values of any size, so no number passes through an integer
//! type that could overflow or through a platform math library; a kernel rounds a result once,
//! when it converts the exact decimal to the nearest binary64 at an arithmetic site.

use std::cmp::Ordering;
use std::fmt::Write;

/// Significant digits Excel shows and reads: a number typed into a cell has at most this many.
const SHOWN_DIGITS: usize = 15;

/// An unsigned integer of any size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Natural {
    /// Little-endian base-2^32 limbs with no high zero limb; zero has none.
    limbs: Vec<u32>,
}

/// The low 32 bits of `value`. The mask keeps the narrowing exact.
fn low32(value: u64) -> u32 {
    (value & 0xFFFF_FFFF) as u32
}

impl Natural {
    pub(crate) fn zero() -> Self {
        Natural { limbs: Vec::new() }
    }

    pub(crate) fn from_u64(value: u64) -> Self {
        let mut natural = Natural {
            limbs: vec![low32(value), low32(value >> 32)],
        };
        natural.normalize();
        natural
    }

    /// 10^exponent.
    pub(crate) fn pow10(exponent: u32) -> Self {
        let mut power = Natural::from_u64(1);
        power.times_pow10(exponent);
        power
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// self * factor + addend.
    fn mul_add_small(&mut self, factor: u32, addend: u32) {
        let mut carry = u64::from(addend);
        for limb in &mut self.limbs {
            let product = u64::from(*limb) * u64::from(factor) + carry;
            *limb = low32(product);
            carry = product >> 32;
        }
        if carry != 0 {
            self.limbs.push(low32(carry));
        }
        self.normalize();
    }

    pub(crate) fn add_one(&mut self) {
        self.mul_add_small(1, 1);
    }

    pub(crate) fn times_pow10(&mut self, mut exponent: u32) {
        while exponent >= 9 {
            self.mul_add_small(1_000_000_000, 0);
            exponent -= 9;
        }
        self.mul_add_small(10u32.pow(exponent), 0);
    }

    fn times_pow5(&mut self, mut exponent: u32) {
        while exponent >= 13 {
            self.mul_add_small(1_220_703_125, 0);
            exponent -= 13;
        }
        self.mul_add_small(5u32.pow(exponent), 0);
    }

    fn times_pow2(&mut self, exponent: u32) {
        if self.is_zero() {
            return;
        }
        let whole = usize::try_from(exponent / 32).expect("a limb count");
        let bits = exponent % 32;
        if bits != 0 {
            let mut carry = 0u32;
            for limb in &mut self.limbs {
                let shifted = (u64::from(*limb) << bits) | u64::from(carry);
                *limb = low32(shifted);
                carry = low32(shifted >> 32);
            }
            if carry != 0 {
                self.limbs.push(carry);
            }
        }
        self.limbs.splice(0..0, std::iter::repeat_n(0, whole));
    }

    /// self + other.
    pub(crate) fn add(&mut self, other: &Natural) {
        if self.limbs.len() < other.limbs.len() {
            self.limbs.resize(other.limbs.len(), 0);
        }
        let mut carry = 0u64;
        for (at, limb) in self.limbs.iter_mut().enumerate() {
            let sum =
                u64::from(*limb) + u64::from(other.limbs.get(at).copied().unwrap_or(0)) + carry;
            *limb = low32(sum);
            carry = sum >> 32;
        }
        if carry != 0 {
            self.limbs.push(low32(carry));
        }
    }

    /// self - other, where other is at most self.
    pub(crate) fn subtract(&mut self, other: &Natural) {
        debug_assert!(*self >= *other, "a natural subtracted from a smaller one");
        let mut borrow = 0u64;
        for (at, limb) in self.limbs.iter_mut().enumerate() {
            let taken = u64::from(other.limbs.get(at).copied().unwrap_or(0)) + borrow;
            let current = u64::from(*limb);
            if current >= taken {
                *limb = low32(current - taken);
                borrow = 0;
            } else {
                *limb = low32((1u64 << 32) + current - taken);
                borrow = 1;
            }
        }
        self.normalize();
    }

    /// self * other.
    pub(crate) fn times(&self, other: &Natural) -> Natural {
        let mut limbs = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (at, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (offset, &b) in other.limbs.iter().enumerate() {
                // At most (2^32-1)^2 + 2*(2^32-1) = 2^64-1: no u64 overflow.
                let sum = u64::from(a) * u64::from(b) + u64::from(limbs[at + offset]) + carry;
                limbs[at + offset] = low32(sum);
                carry = sum >> 32;
            }
            let mut next = at + other.limbs.len();
            while carry != 0 {
                let sum = u64::from(limbs[next]) + carry;
                limbs[next] = low32(sum);
                carry = sum >> 32;
                next += 1;
            }
        }
        let mut product = Natural { limbs };
        product.normalize();
        product
    }

    fn bit_len(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(top) => {
                (self.limbs.len() - 1) * 32
                    + usize::try_from(32 - top.leading_zeros()).expect("a bit count")
            }
        }
    }

    fn bit(&self, at: usize) -> bool {
        (self.limbs[at / 32] >> (at % 32)) & 1 == 1
    }

    /// 2^exponent.
    fn pow2(exponent: u32) -> Self {
        let mut power = Natural::from_u64(1);
        power.times_pow2(exponent);
        power
    }

    fn from_u128(value: u128) -> Self {
        let mut natural = Natural {
            limbs: (0..4)
                .map(|at| low32((value >> (32 * at)) as u64))
                .collect(),
        };
        natural.normalize();
        natural
    }

    /// The value when it is below 2^128.
    fn to_u128(&self) -> Option<u128> {
        (self.limbs.len() <= 4).then(|| {
            self.limbs
                .iter()
                .rev()
                .fold(0u128, |value, &limb| (value << 32) | u128::from(limb))
        })
    }

    /// self * 2 + incoming.
    fn shift_in(&mut self, incoming: bool) {
        self.mul_add_small(2, u32::from(incoming));
    }

    /// self / divisor and self % divisor, for a divisor that is not zero.
    pub(crate) fn div_rem(&self, divisor: &Natural) -> (Natural, Natural) {
        assert!(!divisor.is_zero(), "a natural divided by zero");
        if *self < *divisor {
            return (Natural::zero(), self.clone());
        }
        if let [single] = divisor.limbs[..] {
            let mut quotient = self.clone();
            let remainder = quotient.div_small(single);
            return (quotient, Natural::from_u64(u64::from(remainder)));
        }
        let mut quotient = Natural {
            limbs: vec![0; self.limbs.len()],
        };
        let mut remainder = Natural::zero();
        for at in (0..self.bit_len()).rev() {
            remainder.shift_in(self.bit(at));
            if remainder >= *divisor {
                remainder.subtract(divisor);
                quotient.limbs[at / 32] |= 1 << (at % 32);
            }
        }
        quotient.normalize();
        (quotient, remainder)
    }

    /// Divides self by a non-zero `divisor` in place and returns the remainder.
    fn div_small(&mut self, divisor: u32) -> u32 {
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let current = (remainder << 32) | u64::from(*limb);
            *limb = low32(current / u64::from(divisor));
            remainder = current % u64::from(divisor);
        }
        self.normalize();
        low32(remainder)
    }

    /// The decimal digits, without leading zeros ("0" for zero).
    pub(crate) fn digits(&self) -> String {
        let mut rest = self.clone();
        let mut chunks = Vec::new();
        while !rest.is_zero() {
            chunks.push(rest.div_small(1_000_000_000));
        }
        let mut text = String::new();
        match chunks.split_last() {
            None => text.push('0'),
            Some((top, lower)) => {
                let _ = write!(text, "{top}");
                for chunk in lower.iter().rev() {
                    let _ = write!(text, "{chunk:09}");
                }
            }
        }
        text
    }
}

impl PartialOrd for Natural {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Natural {
    fn cmp(&self, other: &Self) -> Ordering {
        self.limbs
            .len()
            .cmp(&other.limbs.len())
            .then_with(|| self.limbs.iter().rev().cmp(other.limbs.iter().rev()))
    }
}

/// An exact decimal: (-1)^negative * coefficient * 10^exponent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExactDecimal {
    pub(crate) negative: bool,
    pub(crate) coefficient: Natural,
    pub(crate) exponent: i32,
}

/// The direction in which [`ExactDecimal::round_at`] rounds a dropped part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// To the nearer multiple, a half away from zero.
    Nearest,
    /// Away from zero whenever a dropped digit is not zero.
    AwayFromZero,
    /// Toward zero: the dropped digits are removed.
    TowardZero,
}

impl ExactDecimal {
    /// How the rounding and integer functions read a finite number. When its shortest decimal
    /// that converts back to the same binary64 has at most 15 significant digits, the number is
    /// that decimal: every number typed into a cell, which Excel reads to 15 significant digits,
    /// reads back so (2.675, whose binary64 lies below 2.675, reads as 2.675). Any other number is
    /// the exact value of its binary64 (0.1+0.2 reads as 0.3000000000000000444...). Zero of
    /// either sign reads as +0.
    pub(crate) fn read(x: f64) -> ExactDecimal {
        let negative = x < 0.0;
        let magnitude = x.abs();
        if magnitude == 0.0 {
            return ExactDecimal {
                negative: false,
                coefficient: Natural::zero(),
                exponent: 0,
            };
        }
        let shortest = format!("{magnitude:e}");
        let (mantissa, power) = shortest
            .split_once('e')
            .expect("an exponent in the shortest form");
        let power: i32 = power.parse().expect("a decimal exponent");
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.len() + fraction.len() <= SHOWN_DIGITS {
            let coefficient: u64 = format!("{whole}{fraction}")
                .parse()
                .expect("at most 15 decimal digits");
            let scale = i32::try_from(fraction.len()).expect("a short fraction");
            return ExactDecimal {
                negative,
                coefficient: Natural::from_u64(coefficient),
                exponent: power - scale,
            };
        }
        ExactDecimal::binary(x)
    }

    /// How TRUNC, ROUND, ROUNDUP and ROUNDDOWN read a finite number (dc-bd5): [`ExactDecimal::read`]
    /// rounded to 15 significant digits, a half away from zero, with whether that rounding changed
    /// it. Excel 16.0.20430 (cfunc_m3_2's capture): ROUND(2.6749999999999994,2) is 2.68,
    /// TRUNC(2.9999999999999996) is 3, TRUNC(-4503599627370495.5) is -4503599627370500 and
    /// ROUND(9007199254740992,0) is 9007199254740990.
    pub(crate) fn fifteen(x: f64) -> (ExactDecimal, bool) {
        let read = ExactDecimal::read(x);
        let length = i32::try_from(read.coefficient.digits().len()).expect("a digit count");
        let excess = length - i32::try_from(SHOWN_DIGITS).expect("15");
        if excess <= 0 {
            return (read, false);
        }
        match read.round_at(read.exponent + excess, Direction::Nearest) {
            Some(rounded) => (rounded, true),
            None => (read, false),
        }
    }

    /// 1 * 10^exponent.
    pub(crate) fn unit(exponent: i32) -> ExactDecimal {
        ExactDecimal {
            negative: false,
            coefficient: Natural::from_u64(1),
            exponent,
        }
    }

    /// The decimal exponent of a non-zero decimal: the e with 10^e <= |self| < 10^(e+1).
    pub(crate) fn magnitude(&self) -> i32 {
        let length = i32::try_from(self.coefficient.digits().len()).expect("a digit count");
        length - 1 + self.exponent
    }

    /// How ROUND (`ties_up` false) and ROUNDUP, ROUNDDOWN and TRUNC (`ties_up` true) read a
    /// finite non-zero number to fifteen significant digits, where the scaling is exact on the
    /// x87 unit: |x| scaled by 10^n, n = 14 - e for the decimal exponent e of |x|, is rounded
    /// once to 64 significant bits (the x87 unit at 64-bit precision; 10^|n| is exact there for
    /// |n| <= 27), and that value is rounded to an integer, a half up for the directed functions
    /// and down for ROUND. So ROUND(1594628480062905,14) is 1594628480062900 and
    /// ROUNDDOWN(283025097725723.5,0) is 283025097725724, as Excel gives them, and
    /// ROUNDDOWN(53311.02038735315,10), whose 10^10 multiple lies 0.2 * 2^-64 below a half,
    /// reads the half. For |n| > 27 the power is a 64-bit constant, the nearest to 10^|n|, and the
    /// product or quotient is rounded once. The low bit of that constant is not read: of 1,082
    /// bank numbers next to a half at those scales this takes 879, and ROUND's rows lose nothing,
    /// but some constants stand a unit from the nearest. The two tie rules hold there too: 824
    /// directed rows on a tie go up on 726, 70 ROUND rows go down on 68.
    pub(crate) fn fifteen_x87(x: f64, ties_up: bool) -> Option<ExactDecimal> {
        let shift = 14 - ExactDecimal::binary(x).magnitude();
        let bits = x.abs().to_bits();
        let biased = i32::try_from((bits >> 52) & 0x7FF).expect("an 11-bit exponent");
        let fraction_bits = bits & ((1u64 << 52) - 1);
        let (mantissa, exponent) = if biased == 0 {
            (fraction_bits, -1074)
        } else {
            (fraction_bits | (1u64 << 52), biased - 1075)
        };
        let mut numerator = Natural::from_u64(mantissa);
        let mut denominator = Natural::from_u64(1);
        if shift >= 0 {
            numerator.times_pow10(shift.unsigned_abs());
        } else {
            denominator = Natural::pow10(shift.unsigned_abs());
        }
        // The scaled value rounded to 64 bits: kept * 2^power.
        let (kept, power) = if shift.abs() <= 27 {
            round_to_64_bits(&numerator, exponent, &denominator)
        } else {
            let one = Natural::from_u64(1);
            let (scale, scale_power) = round_to_64_bits(&Natural::pow10(shift.unsigned_abs()), 0, &one);
            let scale = Natural::from_u128(scale);
            let start = Natural::from_u64(mantissa);
            if shift > 0 {
                round_to_64_bits(&start.times(&scale), exponent + scale_power, &one)
            } else {
                round_to_64_bits(&start, exponent - scale_power, &scale)
            }
        };
        let whole = if power >= 0 {
            let mut whole = Natural::from_u128(kept);
            whole.times_pow2(power.unsigned_abs());
            whole
        } else {
            let drop = power.unsigned_abs();
            let (whole, rest) = if drop >= 128 {
                (0, kept)
            } else {
                (kept >> drop, kept - ((kept >> drop) << drop))
            };
            let half = if drop > 128 {
                u128::MAX
            } else {
                1u128 << (drop - 1)
            };
            let up = rest > half || (rest == half && ties_up);
            Natural::from_u128(whole + u128::from(up))
        };
        Some(
            ExactDecimal {
                negative: x < 0.0,
                coefficient: whole,
                exponent: -shift,
            }
            .normalized(),
        )
    }

    /// The same value with no trailing zero in its coefficient.
    pub(crate) fn normalized(mut self) -> ExactDecimal {
        while !self.coefficient.is_zero() {
            let mut shorter = self.coefficient.clone();
            if shorter.div_small(10) != 0 {
                break;
            }
            self.coefficient = shorter;
            self.exponent += 1;
        }
        self
    }

    /// Whether this decimal is exactly the value of the finite binary64 `x`.
    pub(crate) fn is_value_of(&self, x: f64) -> bool {
        let this = self.clone().normalized();
        let that = ExactDecimal::binary(x).normalized();
        this.coefficient == that.coefficient
            && (this.coefficient.is_zero()
                || (this.exponent == that.exponent && this.negative == that.negative))
    }

    /// The exact value of a finite binary64, whatever decimal it shows. Zero of either sign is +0.
    pub(crate) fn binary(x: f64) -> ExactDecimal {
        let negative = x < 0.0;
        // The binary64 is mantissa * 2^exponent exactly, with an integer mantissa below 2^53.
        let bits = x.abs().to_bits();
        let biased = i32::try_from((bits >> 52) & 0x7FF).expect("an 11-bit exponent");
        let fraction_bits = bits & ((1u64 << 52) - 1);
        let (mantissa, exponent) = if biased == 0 {
            (fraction_bits, -1074)
        } else {
            (fraction_bits | (1u64 << 52), biased - 1075)
        };
        let mut coefficient = Natural::from_u64(mantissa);
        if exponent >= 0 {
            coefficient.times_pow2(exponent.unsigned_abs());
            ExactDecimal {
                negative,
                coefficient,
                exponent: 0,
            }
        } else {
            // m * 2^-k = m * 5^k * 10^-k.
            coefficient.times_pow5(exponent.unsigned_abs());
            ExactDecimal {
                negative,
                coefficient,
                exponent,
            }
        }
    }

    /// The decimal as text that Rust's binary64 parser reads: `[-]digits e exponent`.
    pub(crate) fn text(&self) -> String {
        format!(
            "{}{}e{}",
            if self.negative { "-" } else { "" },
            self.coefficient.digits(),
            self.exponent
        )
    }

    /// This decimal rounded in `direction` to a multiple of 10^position, or `None` when it has
    /// no digit below that position (it is already such a multiple, unchanged).
    pub(crate) fn round_at(&self, position: i32, direction: Direction) -> Option<ExactDecimal> {
        if self.coefficient.is_zero() || position <= self.exponent {
            return None;
        }
        let dropped = position.abs_diff(self.exponent);
        let length = self.coefficient.digits().len();
        let (mut kept, bump) = if usize::try_from(dropped).expect("a digit count") > length {
            // Every digit lies below the position and the leading one more than one place
            // below it: the dropped part is not zero and is less than half a unit.
            (Natural::zero(), direction == Direction::AwayFromZero)
        } else {
            let unit = Natural::pow10(dropped);
            let (kept, dropped_part) = self.coefficient.div_rem(&unit);
            let bump = match direction {
                Direction::Nearest => {
                    let mut twice = dropped_part.clone();
                    twice.add(&dropped_part);
                    twice >= unit
                }
                Direction::AwayFromZero => !dropped_part.is_zero(),
                Direction::TowardZero => false,
            };
            (kept, bump)
        };
        if bump {
            kept.add_one();
        }
        Some(ExactDecimal {
            negative: self.negative,
            coefficient: kept,
            exponent: position,
        })
    }
}

/// numerator * 2^exponent / denominator rounded to 64 significant bits, to the nearest and a
/// tie to even, as `(kept, power)` with the value kept * 2^power and kept below 2^64.
fn round_to_64_bits(numerator: &Natural, exponent: i32, denominator: &Natural) -> (u128, i32) {
    // Scale so the integer quotient has at least 66 bits.
    let scale = (66 + denominator.bit_len()).saturating_sub(numerator.bit_len());
    let mut scaled = numerator.clone();
    scaled.times_pow2(u32::try_from(scale).expect("a bit count"));
    let (quotient, remainder) = scaled.div_rem(denominator);
    let drop = u32::try_from(quotient.bit_len() - 64).expect("a bit count");
    let (high, low) = quotient.div_rem(&Natural::pow2(drop));
    let mut kept = high.to_u128().expect("64 bits");
    let up = match low.cmp(&Natural::pow2(drop - 1)) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => !remainder.is_zero() || kept & 1 == 1,
    };
    let mut power = exponent - i32::try_from(scale).expect("a bit count")
        + i32::try_from(drop).expect("a bit count");
    if up {
        kept += 1;
        if kept == 1u128 << 64 {
            kept >>= 1;
            power += 1;
        }
    }
    (kept, power)
}

/// The integer part of a binary64, toward zero, found from its bits: exact on every target.
pub(crate) fn truncate(x: f64) -> f64 {
    let bits = x.to_bits();
    let unbiased = i32::try_from((bits >> 52) & 0x7FF).expect("an 11-bit exponent") - 1023;
    if unbiased >= 52 {
        // Every binary64 of this size is an integer (or not finite).
        x
    } else if unbiased < 0 {
        // Below 1 in magnitude: the integer part is a zero of the number's sign.
        f64::from_bits(bits & (1u64 << 63))
    } else {
        let fraction_mask = (1u64 << (52 - unbiased.unsigned_abs())) - 1;
        f64::from_bits(bits & !fraction_mask)
    }
}

/// The largest integer not above `x`: exact, since a binary64 below 2^52 in magnitude that is
/// not an integer has an integer part one step from it.
pub(crate) fn floor(x: f64) -> f64 {
    let whole = truncate(x);
    if whole > x { whole - 1.0 } else { whole }
}

/// The smallest integer not below `x`.
pub(crate) fn ceil(x: f64) -> f64 {
    let whole = truncate(x);
    if whole < x { whole + 1.0 } else { whole }
}

/// Unit tests of the exact arithmetic on its own; the functions that use it are tested through
/// calc_func::invoke by the cfunc-m3.2 acceptance cases.
#[cfg(test)]
mod tests {
    use super::*;

    fn natural(text: &str) -> Natural {
        let mut n = Natural::zero();
        for digit in text.bytes() {
            n.mul_add_small(10, u32::from(digit - b'0'));
        }
        n
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn naturals_divide_exactly_at_any_size() {
        let big = natural("123456789012345678901234567890123456789012345678901234567890");
        let divisor = natural("98765432109876543210987");
        let (quotient, remainder) = big.div_rem(&divisor);
        assert_eq!(quotient.digits(), "1249999988609375000142391093749550070");
        assert_eq!(remainder.digits(), "29599966484956903948800");
        let mut back = quotient.clone();
        let mut product = Natural::zero();
        // quotient * divisor + remainder, by repeated doubling.
        let mut addend = divisor.clone();
        while !back.is_zero() {
            if back.div_small(2) == 1 {
                product.add(&addend);
            }
            let copy = addend.clone();
            addend.add(&copy);
        }
        product.add(&remainder);
        assert_eq!(product, big);
        assert_eq!(Natural::pow10(30).digits(), format!("1{}", "0".repeat(30)));
        let (q, r) = natural("1000").div_rem(&natural("7"));
        assert_eq!((q.digits(), r.digits()), ("142".to_owned(), "6".to_owned()));
        assert_eq!(
            natural("123456789012345678901234567890")
                .times(&natural("98765432109876543210"))
                .digits(),
            "12193263113702179522496570642237463801111263526900"
        );
        assert_eq!(
            natural("4294967295").times(&natural("4294967295")).digits(),
            "18446744065119617025"
        );
        assert!(natural("5").times(&Natural::zero()).is_zero());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_number_reads_as_its_shown_decimal_or_its_binary64() {
        assert_eq!(ExactDecimal::read(2.675).text(), "2675e-3");
        assert_eq!(ExactDecimal::read(-0.0).text(), "0e0");
        assert_eq!(ExactDecimal::read(1e300).text(), "1e300");
        assert_eq!(
            ExactDecimal::read(0.1 + 0.2).text(),
            "300000000000000044408920985006261616945266723632812500e-54"
        );
        assert_eq!(
            ExactDecimal::read(1_152_921_504_606_846_976.0).text(),
            "1152921504606846976e0"
        );
        // The exact binary64 value, whatever decimal the number shows.
        assert_eq!(
            ExactDecimal::binary(2.675).text(),
            "2674999999999999822364316059974953532218933105468750e-51"
        );
        assert_eq!(
            ExactDecimal::binary(1e23).text(),
            "99999999999999991611392e0"
        );
        assert_eq!(ExactDecimal::binary(-0.0).coefficient, Natural::zero());
        assert!(!ExactDecimal::binary(-0.0).negative);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn integer_parts_come_from_the_bits() {
        for (x, t, f, c) in [
            (2.5, 2.0, 2.0, 3.0),
            (-2.5, -2.0, -3.0, -2.0),
            (0.25, 0.0, 0.0, 1.0),
            (-0.25, -0.0, -1.0, -0.0),
            (
                4_503_599_627_370_495.5,
                4_503_599_627_370_495.0,
                4_503_599_627_370_495.0,
                4_503_599_627_370_496.0,
            ),
            (f64::MAX, f64::MAX, f64::MAX, f64::MAX),
        ] {
            assert_eq!(truncate(x).to_bits(), f64::to_bits(t), "truncate {x}");
            assert_eq!(floor(x).to_bits(), f64::to_bits(f), "floor {x}");
            assert_eq!(ceil(x).to_bits(), f64::to_bits(c), "ceil {x}");
        }
    }
}

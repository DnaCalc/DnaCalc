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

    /// The exact rational rounded to a 64-bit binary significand. Keep that
    /// binary value as an exact decimal so the arithmetic site can publish it.
    pub(crate) fn rounded_pc64(&self) -> ExactDecimal {
        if self.coefficient.is_zero() {
            return self.clone();
        }
        let (kept, power) = self.pc64_parts();
        Self::from_binary_parts(self.negative, kept, power)
    }

    fn pc64_parts(&self) -> (u128, i32) {
        let mut numerator = self.coefficient.clone();
        let denominator = if self.exponent < 0 {
            Natural::pow10(self.exponent.unsigned_abs())
        } else {
            numerator.times_pow10(self.exponent.unsigned_abs());
            Natural::from_u64(1)
        };
        round_to_64_bits(&numerator, 0, &denominator)
    }

    fn from_binary_parts(negative: bool, kept: u128, power: i32) -> ExactDecimal {
        let mut coefficient = Natural::from_u128(kept);
        let exponent = if power >= 0 {
            coefficient.times_pow2(power.unsigned_abs());
            0
        } else {
            coefficient.times_pow5(power.unsigned_abs());
            power
        };
        ExactDecimal {
            negative,
            coefficient,
            exponent,
        }
    }

    /// Decimal publication scales the normalized integer coefficient by signed
    /// powers of ten, taking the low four exponent bits first. Each table
    /// operand and multiplication has a 64-bit binary significand.
    pub(crate) fn scaled_pc64(&self) -> ExactDecimal {
        self.clone().normalized().scaled_pc64_at_exponent()
    }

    /// Like [`ExactDecimal::scaled_pc64`], but keeps `self.exponent` without
    /// stripping trailing decimal zeros from `self.coefficient`. Directed
    /// truncation (ROUNDDOWN, TRUNC and ROUNDUP's lower base) scales the
    /// truncated integer directly at `-num_digits` (dc-m53ln: Excel 16.0.20527,
    /// 24/24 fresh frozen discriminators each for ROUNDDOWN, TRUNC and ROUNDUP,
    /// vs 0/24 for ROUND which normalizes first; fixes ROUNDDOWN/ROUNDUP on
    /// ±2.1000001390671165e-302 at 307 places).
    pub(crate) fn scaled_pc64_at_exponent(&self) -> ExactDecimal {
        if self.coefficient.is_zero() {
            return self.clone();
        }
        let mut magnitude = self.exponent.unsigned_abs();
        let mut shift = 0;
        let (mut kept, mut power) =
            round_to_64_bits(&self.coefficient, 0, &Natural::from_u64(1));
        while magnitude != 0 {
            let part = (magnitude & 15) << shift;
            if part != 0 {
                let exponent = i32::try_from(part).expect("a decimal exponent");
                let factor = Self::unit(if self.exponent < 0 {
                    -exponent
                } else {
                    exponent
                });
                let (factor_kept, factor_power) = factor.pc64_parts();
                (kept, power) = round_to_64_bits(
                    &Natural::from_u128(kept * factor_kept),
                    power + factor_power,
                    &Natural::from_u64(1),
                );
            }
            magnitude >>= 4;
            shift += 4;
        }
        Self::from_binary_parts(self.negative, kept, power)
    }

    /// The decimal exponent of a non-zero decimal: the e with 10^e <= |self| < 10^(e+1).
    pub(crate) fn magnitude(&self) -> i32 {
        let length = i32::try_from(self.coefficient.digits().len()).expect("a digit count");
        length - 1 + self.exponent
    }

    /// How ROUND (`ties_up` false) and ROUNDUP, ROUNDDOWN, TRUNC and COMPLEX (`ties_up` true)
    /// read a finite non-zero number to fifteen significant digits.
    ///
    /// ROUND (`ties_up` false; dc-j9k.88 / cb17) delegates to Excel's shared Q0.64 two-branch
    /// 15-digit reader (`number_text::decimal`), matching `&`, `TEXT`, `FIXED`, `DOLLAR`,
    /// `AMORDEGRC`, and `_EUROTOOL.EUROCONVERT`: exact Q16.64 split on `[1, 2^16)`, and
    /// `ceil(frexp_k / 3.3219)` low-nibble PC=64 scaling with truncating shift into Q0.64
    /// outside `[1, 2^16)`.
    ///
    /// The directed functions and complex text (`ties_up` true; dc-m53ln, dc-ymjf.2.128,
    /// dc-j9k.88) estimate the decimal exponent from the IEEE `frexp` exponent
    /// `k = floor(log2(|x|)) + 1` as `e_est = floor(k / 3.3219)` (every constant in the Farey
    /// interval `(146/485, 233/774)` produces the same integer for all binary64 exponents,
    /// confirmed at `k = ±485, -970, +970, +774` and 29/29 critical exponents), scale `|x|` by
    /// `s1 = 14 - e_est` using the low-nibble-first 64-bit chain `V = RN64(V * RN64(10^+-c))` at
    /// every shift (including `|s1| <= 27`, 8/8 fresh discriminators), multiply by 10 while below
    /// `10^14`, and round to an integer with a half up. In the rare sliver where `e_est < e`
    /// (`k = -485` and `k = -970` with `|x| >= 10^-146` or `10^-292`), `V >= 10^15` rounds to a
    /// 16-digit integer `M`, publishes `M * 10^-s1` via the nibble chain to binary64 `y`, and
    /// re-scales `y` by `s1 - 1` (150/150 at `k = -485`).
    pub(crate) fn fifteen_x87(x: f64, ties_up: bool) -> Option<ExactDecimal> {
        if !ties_up {
            return Some(crate::number_text::decimal(x));
        }
        let bits = x.abs().to_bits();
        let biased = i32::try_from((bits >> 52) & 0x7FF).expect("an 11-bit exponent");
        let fraction_bits = bits & ((1u64 << 52) - 1);
        let (mantissa, exponent) = if biased == 0 {
            (fraction_bits, -1074)
        } else {
            (fraction_bits | (1u64 << 52), biased - 1075)
        };
        Some(directed_fifteen_x87(x < 0.0, mantissa, exponent))
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

/// Round the positive 64-bit value `kept * 2^power` to an integer `Natural`,
/// breaking exact halves up when `ties_up` is true and down when false.
fn round_64_to_natural(kept: u128, power: i32, ties_up: bool) -> Natural {
    if power >= 0 {
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
    }
}

/// Whether `kept * 2^power >= bound` for positive `kept < 2^64` and `bound < 2^64`.
fn ge_int_64(kept: u128, power: i32, bound: u128) -> bool {
    if power >= 0 {
        let shift = power.unsigned_abs();
        shift >= 64 || kept.checked_shl(shift).is_none_or(|value| value >= bound)
    } else {
        let drop = power.unsigned_abs();
        drop < 128 && (kept >> drop) >= bound
    }
}

/// Multiply a positive 64-bit value `(kept, power)` by `10` with 64-bit RN-even rounding.
fn mul_ten_64(kept: u128, power: i32) -> (u128, i32) {
    let one = Natural::from_u64(1);
    round_to_64_bits(&Natural::from_u128(kept * 10), power, &one)
}

/// Directed-function and complex-coefficient 15-digit initial reading (dc-m53ln, dc-ymjf.2.128):
/// estimate the decimal exponent from `frexp`'s exponent `k = floor(log2(|x|)) + 1` as
/// `floor(k / 3.3219)`, scale `|x|` by `14 - e_est` via the low-nibble-first 64-bit chain,
/// multiply by 10 while below `10^14`, and if the scaled value reaches `10^15` (the `0.1%` sliver
/// at `k = -485, -970`) round to 16 digits, publish back to binary64 via the nibble chain and
/// re-scale by `shift - 1`.
fn directed_fifteen_x87(negative: bool, mantissa: u64, exponent: i32) -> ExactDecimal {
    const TEN_14: u128 = 100_000_000_000_000;
    const TEN_15: u128 = 1_000_000_000_000_000;

    let frexp_exp = exponent + i32::try_from(64 - mantissa.leading_zeros()).expect("u64 bit len");
    let e_est = (frexp_exp * 10_000).div_euclid(33_219);
    let mut shift = 14 - e_est;
    let (mut kept, mut power) = nibble_chain(mantissa, exponent, shift);
    while !ge_int_64(kept, power, TEN_14) {
        (kept, power) = mul_ten_64(kept, power);
        shift += 1;
    }
    if ge_int_64(kept, power, TEN_15) {
        let wide = ExactDecimal {
            negative: false,
            coefficient: round_64_to_natural(kept, power, true),
            exponent: -shift,
        };
        if let Ok(published) = wide.scaled_pc64_at_exponent().text().parse::<f64>()
            && published.is_finite()
            && published != 0.0
        {
            let bits = published.to_bits();
            let biased = i32::try_from((bits >> 52) & 0x7FF).expect("an 11-bit exponent");
            let frac = bits & ((1u64 << 52) - 1);
            let (m2, e2) = if biased == 0 {
                (frac, -1074)
            } else {
                (frac | (1u64 << 52), biased - 1075)
            };
            shift -= 1;
            let (k2, p2) = nibble_chain(m2, e2, shift);
            return ExactDecimal {
                negative,
                coefficient: round_64_to_natural(k2, p2, false),
                exponent: -shift,
            }
            .normalized();
        }
    }
    ExactDecimal {
        negative,
        coefficient: round_64_to_natural(kept, power, true),
        exponent: -shift,
    }
    .normalized()
}

/// numerator * 2^exponent / denominator rounded to 64 significant bits, to the nearest and a
/// tie to even, as `(kept, power)` with the value kept * 2^power and kept below 2^64.
/// The nibble chain on `mantissa * 2^exponent`: for each non-zero 4-bit part c of |shift|,
/// low part first, V = RN64(V * RN64(10^+-c)) (a multiplication for a negative shift too).
fn nibble_chain(mantissa: u64, exponent: i32, shift: i32) -> (u128, i32) {
    let one = Natural::from_u64(1);
    let mut kept = u128::from(mantissa);
    let mut power = exponent;
    let mut magnitude = shift.unsigned_abs();
    let mut place = 0;
    while magnitude != 0 {
        let part = (magnitude & 15) << place;
        if part != 0 {
            let part = i32::try_from(part).expect("a decimal exponent");
            let (factor, factor_power) =
                ExactDecimal::unit(if shift < 0 { -part } else { part }).pc64_parts();
            (kept, power) =
                round_to_64_bits(&Natural::from_u128(kept * factor), power + factor_power, &one);
        }
        magnitude >>= 4;
        place += 4;
    }
    (kept, power)
}

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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn directed_fifteen_uses_frexp_estimate_and_unnormalized_publication() {
        // Bank rows where frexp exponent estimate e_est = floor((e2+1)/3.3219) exceeds exact e10
        // by 1, so the initial nibble-chain scale is 13 - e10 followed by * 10 (dc-m53ln).
        for (bits, expected_text) in [
            (0x5e33_4e95_8ab2_6e0eu64, "602715035061692e131"), // e2=484, k=485 (Farey boundary)
            (0x2873_8c74_d9ee_a6f1u64, "79381371240304e-127"), // shift=+128 -> 127 then *10
            (0x7f73_5621_d584_db16u64, "848657475319565e291"), // shift=-291 -> -292 then *10
            (0x1982_d0b8_aadc_242bu64, "864856875179571e-200"), // shift=+200 -> 199 then *10
            // v0 >= 10^15 rescan sliver at e2=-486 (k=-485, e_est=-147 < e10=-146):
            (0x219f_f77a_4c10_c994u64, "100000014705586e-160"),
            (0x219f_f77a_4c10_c9e6u64, "100000014705588e-160"),
        ] {
            let x = f64::from_bits(bits);
            let reading = ExactDecimal::fifteen_x87(x, true).expect("directed reading");
            assert_eq!(reading.text(), expected_text, "bits=0x{bits:016x}");
        }

        // Unnormalized publication for directed truncation vs normalized for ROUND:
        // ROUNDDOWN(2.1000001390671165e-302, 307) keeps 210000 * 10^-307 -> 0x014ccd59caa6a750,
        // while ROUND normalizes to 21 * 10^-303 -> 0x014ccd59caa6a751.
        let down = ExactDecimal {
            negative: false,
            coefficient: Natural::from_u64(210_000),
            exponent: -307,
        };
        let unnorm: f64 = down.scaled_pc64_at_exponent().text().parse().unwrap();
        let norm: f64 = down.scaled_pc64().text().parse().unwrap();
        assert_eq!(unnorm.to_bits(), 0x014c_cd59_caa6_a750);
        assert_eq!(norm.to_bits(), 0x014c_cd59_caa6_a751);
    }
}

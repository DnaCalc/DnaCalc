//! The comparison family fragment: the comparison operators and their kernels.
//!
//! A comparison coerces neither operand. An operand's error is the result, the left operand's
//! first. Values of different kinds order as numbers, then text, then logicals (FALSE before
//! TRUE). A blank operand (Empty, or Missing) compares as the empty value of the other operand's
//! kind (0, empty text or FALSE), and two blanks are equal.
//!
//! Two numbers compare as Excel compares them: each is rounded to 15 significant digits, a tie at
//! the 16th digit away from zero, and two numbers with the same rounding are equal (0.1+0.2 equals
//! 0.3, and 1+1E-15 equals 1); otherwise they order by value. Equal numbers are equal without any
//! rounding. Two different numbers are first tested for nearness, which is an arithmetic site: it
//! rounds a difference and a product under the call's numeric mode, so under Exact, which has no
//! implementation, the comparison is refused with ExactUnavailable. Under Fast, numbers farther
//! apart than any two with the same rounding order by value, and nearer ones by their roundings,
//! each found exactly from the number's decimal value. Where Excel's own rounding departs from the
//! exact one, a comparison that the departure could decide is refused as not modelled, naming no
//! argument (bead dc-j9k owns Excel's rule there): a number within a relative 2^-59 of a
//! 16th-digit tie but not on it, a number of binary64's top binade (from 2^1023), and a number
//! from 1E-146 or 1E-292 to a relative 2^-8 above it.
//!
//! Two texts compare as [`collation::compare`] states: Excel's own pass over ASCII, then Windows'
//! comparison under the locale en-ZA ignoring case, over every code unit (dc-zsn.1, dc-zsn.2).
//! Texts with the same code units are equal. Text the collation builds no weights for (see
//! [`collation`]: U+0000, the Hangul conjoining jamo, U+30A0, the isolated and medial forms of
//! Arabic marks, iteration outside the measured adjacent punctuation contexts) is refused as not modelled,
//! naming the operand that holds it ([`Beyond`]), unless the texts are the same or an identical
//! complete modern Jamo prefix cancels with independent known suffixes.

mod collation;
mod root_collation;
mod sort_key;
mod windows_collation;

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::coerce;
use crate::decimal::{ExactDecimal, Natural};
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreRef};
use std::cmp::Ordering;

const CODE: u16 = 2;

const EQUAL: FunctionId = FunctionId::new(CODE, 1);
const NOT_EQUAL: FunctionId = FunctionId::new(CODE, 2);
const LESS: FunctionId = FunctionId::new(CODE, 3);
const LESS_OR_EQUAL: FunctionId = FunctionId::new(CODE, 4);
const GREATER: FunctionId = FunctionId::new(CODE, 5);
const GREATER_OR_EQUAL: FunctionId = FunctionId::new(CODE, 6);

const VALUES: &[Parameter] = &[Parameter::value("value1"), Parameter::value("value2")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry::operator(EQUAL, "=", Notation::Infix, VALUES, equal),
        CatalogueEntry::operator(NOT_EQUAL, "<>", Notation::Infix, VALUES, not_equal),
        CatalogueEntry::operator(LESS, "<", Notation::Infix, VALUES, less),
        CatalogueEntry::operator(LESS_OR_EQUAL, "<=", Notation::Infix, VALUES, less_or_equal),
        CatalogueEntry::operator(GREATER, ">", Notation::Infix, VALUES, greater),
        CatalogueEntry::operator(
            GREATER_OR_EQUAL,
            ">=",
            Notation::Infix,
            VALUES,
            greater_or_equal,
        ),
    ],
};

/// The share of the larger magnitude beyond which two numbers' differences are too wide for them
/// to have the same 15-digit rounding: two such roundings lie within 1E-14 of it, and the margin
/// covers the rounding of the difference and of the product.
const APART: f64 = 2e-14;

/// Significant digits of the rounding.
const DIGITS: usize = 15;

/// The smallest magnitude of binary64's top binade, 2^1023.
const TOP_BINADE: f64 = f64::from_bits(0x7FE0_0000_0000_0000);

/// The magnitudes Excel rounds one digit finer than their own: from 1E-146 and 1E-292 (each less a
/// relative 2^-28 of margin) to a relative 2^-8 above each.
const FINER: [(f64, f64); 2] = [
    (9.999999962747096e-147, 1.00390625e-146),
    (9.999999962747096e-293, 1.00390625e-292),
];

/// A number within a relative 2^-NEAR_TIE_BITS of a 16th-digit tie, but not on it, may be rounded
/// by Excel to either side of it.
const NEAR_TIE_BITS: u32 = 59;

/// What a comparison asks of two texts: whether they are equal, or how they order.
#[derive(Clone, Copy)]
pub(super) enum Want {
    Equality,
    Order,
}

/// Why [`texts`] does not decide two texts: `operand` (0 for the first, 1 for the second) holds a
/// code unit the collation builds no weights for. The comparison operators refuse the pair as not
/// modelled, naming that operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Beyond {
    pub(super) operand: usize,
}

fn equal(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(EQUAL, arguments, services, Want::Equality, Ordering::is_eq)
}

fn not_equal(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(
        NOT_EQUAL,
        arguments,
        services,
        Want::Equality,
        Ordering::is_ne,
    )
}

fn less(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(LESS, arguments, services, Want::Order, Ordering::is_lt)
}

fn less_or_equal(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(
        LESS_OR_EQUAL,
        arguments,
        services,
        Want::Order,
        Ordering::is_le,
    )
}

fn greater(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(GREATER, arguments, services, Want::Order, Ordering::is_gt)
}

fn greater_or_equal(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    compare(
        GREATER_OR_EQUAL,
        arguments,
        services,
        Want::Order,
        Ordering::is_ge,
    )
}

fn compare(
    id: FunctionId,
    arguments: &[CalcValue],
    services: &EvalServices<'_>,
    want: Want,
    holds: fn(Ordering) -> bool,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(id, services);
    Site::finish(order(&site, arguments, want).map(|ordering| CalcValue::logical(holds(ordering))))
}

/// A scalar operand as a comparison sees it.
#[derive(Clone, Copy)]
enum Operand<'a> {
    Number(f64),
    Overflow(f64),
    Text(&'a [u16]),
    Logical(bool),
    Blank,
}

impl Operand<'_> {
    /// The order of kinds: numbers, then text, then logicals.
    fn kind(self) -> u8 {
        match self {
            Operand::Number(_) | Operand::Overflow(_) | Operand::Blank => 0,
            Operand::Text(_) => 1,
            Operand::Logical(_) => 2,
        }
    }

    /// The empty value of this operand's kind, which a blank compares as.
    fn blank(self) -> Self {
        match self {
            Operand::Number(_) | Operand::Overflow(_) => Operand::Number(0.0),
            Operand::Text(_) => Operand::Text(&[]),
            Operand::Logical(_) => Operand::Logical(false),
            Operand::Blank => Operand::Blank,
        }
    }
}

pub(super) fn order(site: &Site<'_>, arguments: &[CalcValue], want: Want) -> Step<Ordering> {
    site.scalar_operands(arguments)?;
    let mut operands = [Operand::Blank; 2];
    for (index, argument) in arguments.iter().enumerate() {
        operands[index] = match argument.core_ref() {
            CoreRef::Number(n) if argument.is_excel_overflow() => Operand::Overflow(n),
            CoreRef::Number(n) => Operand::Number(site.argument(index, coerce::operand(n))?),
            CoreRef::Text(text) => Operand::Text(text.utf16_code_units()),
            CoreRef::Logical(b) => Operand::Logical(b),
            CoreRef::Empty | CoreRef::Missing => Operand::Blank,
            CoreRef::Error(code) => return Err(Exit::Error(code)),
            CoreRef::Array(_) | CoreRef::Reference(_) => {
                return Err(site.not_modelled(Some(index)));
            }
        };
    }
    let (left, right) = match (operands[0], operands[1]) {
        (Operand::Blank, Operand::Blank) => return Ok(Ordering::Equal),
        (Operand::Blank, other) => (other.blank(), other),
        (other, Operand::Blank) => (other, other.blank()),
        pair => pair,
    };
    match (left, right) {
        (Operand::Number(a), Operand::Number(b)) => numbers(site, a, b),
        (Operand::Overflow(a), Operand::Number(b))
        | (Operand::Number(a), Operand::Overflow(b))
        | (Operand::Overflow(a), Operand::Overflow(b)) => overflow_numbers(site, a, b),
        (Operand::Text(a), Operand::Text(b)) => {
            texts(a, b, want).map_err(|beyond| site.not_modelled(Some(beyond.operand)))
        }
        (Operand::Logical(a), Operand::Logical(b)) => Ok(a.cmp(&b)),
        (a, b) => Ok(a.kind().cmp(&b.kind())),
    }
}

/// The order of qualified overflow encodings and already-validated numeric operands.
/// Callers must retain the CalcValue qualification; arbitrary NaNs are not admitted.
pub(super) fn overflow_numbers(site: &Site<'_>, a: f64, b: f64) -> Step<Ordering> {
    if !(a.is_nan() || b.is_nan()) {
        return numbers(site, a, b);
    }
    if a.to_bits() == b.to_bits() {
        return Ok(Ordering::Equal);
    }
    if a.is_sign_negative() != b.is_sign_negative() {
        return Ok(a.total_cmp(&b));
    }
    let half = |number: f64| {
        if number.is_nan() {
            f64::from_bits(number.to_bits() - (1_u64 << 52))
        } else {
            number * 0.5
        }
    };
    numbers(site, half(a), half(b))
}

/// The order of two ordinary finite numbers. Equal numbers need no rounding. For different numbers the
/// nearness test computes their difference and [`APART`] of the larger magnitude at the call's
/// arithmetic sites, which round under its numeric mode. Numbers farther apart order by value;
/// nearer ones by their 15-digit roundings, refused ([`in_band`]) where Excel's rounding may
/// depart from the exact one and the departure could decide the comparison.
pub(super) fn numbers(site: &Site<'_>, a: f64, b: f64) -> Step<Ordering> {
    if !a.is_finite() || !b.is_finite() {
        return Err(site.not_modelled(None));
    }
    if a == b {
        return Ok(Ordering::Equal);
    }
    let difference = site.subtract(a, b)?.abs();
    let apart = site.multiply(a.abs().max(b.abs()), APART)?;
    if difference > apart || a.is_sign_negative() != b.is_sign_negative() {
        return Ok(a.total_cmp(&b));
    }
    let (Some(x), Some(y)) = (Roundings::of(a.abs()), Roundings::of(b.abs())) else {
        return Err(in_band(site));
    };
    match (x.other, y.other) {
        (None, None) if x.nearest == y.nearest => Ok(Ordering::Equal),
        (None, None) => Ok(a.total_cmp(&b)),
        _ if x.disjoint(&y) => Ok(a.total_cmp(&b)),
        _ => Err(in_band(site)),
    }
}

/// The refusal of two nearly equal numbers in a band where Excel's rounding departs from the
/// exact one (a near tie of [`NEAR_TIE_BITS`], [`TOP_BINADE`], [`FINER`]): not modelled, naming no
/// argument, since the pair decides it. Bead dc-j9k owns Excel's rule in these bands.
fn in_band(site: &Site<'_>) -> Exit {
    site.not_modelled(None)
}

/// A magnitude rounded to [`DIGITS`] significant digits: the digits as an integer of exactly that
/// many digits (0 for zero), and the exponent of the leading digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rounded {
    digits: u64,
    exponent: i32,
}

impl Rounded {
    /// The next rounding up: one more in the last digit, carrying into a new leading digit.
    fn up(self) -> Rounded {
        let digits = self.digits + 1;
        if digits == 1_000_000_000_000_000 {
            Rounded {
                digits: 100_000_000_000_000,
                exponent: self.exponent + 1,
            }
        } else {
            Rounded { digits, ..self }
        }
    }
}

/// How Excel rounds a magnitude: the nearest rounding (a tie away from zero) and, for a magnitude
/// near a 16th-digit tie but not on it, the rounding on the tie's other side, which Excel may give.
#[derive(Clone, Copy, Debug)]
struct Roundings {
    nearest: Rounded,
    other: Option<Rounded>,
}

impl Roundings {
    /// The roundings of a finite magnitude, or `None` where Excel rounds it one digit finer
    /// ([`FINER`]) or by a rule not modelled ([`TOP_BINADE`]).
    fn of(magnitude: f64) -> Option<Roundings> {
        if magnitude >= TOP_BINADE
            || FINER
                .iter()
                .any(|&(low, high)| (low..=high).contains(&magnitude))
        {
            return None;
        }
        if magnitude == 0.0 {
            let zero = Rounded {
                digits: 0,
                exponent: 0,
            };
            return Some(Roundings {
                nearest: zero,
                other: None,
            });
        }
        let exact = ExactDecimal::binary(magnitude);
        let all = exact.coefficient.digits();
        let length = all.len();
        let exponent = i32::try_from(length).expect("a digit count") - 1 + exact.exponent;
        let kept = &all[..length.min(DIGITS)];
        let digits = kept.parse::<u64>().expect("decimal digits")
            * 10u64.pow(u32::try_from(DIGITS - kept.len()).expect("a digit count"));
        let down = Rounded { digits, exponent };
        if length <= DIGITS {
            return Some(Roundings {
                nearest: down,
                other: None,
            });
        }
        // With r the dropped part of the m dropped digits, the tie is at r = 10^m / 2: the number
        // rounds up from it, and is near it when |2r - 10^m| * 2^(NEAR_TIE_BITS - 1) is below the
        // coefficient.
        let dropped = u32::try_from(length - DIGITS).expect("a digit count");
        let power = Natural::pow10(dropped);
        let (_, remainder) = exact.coefficient.div_rem(&power);
        let mut twice = remainder.clone();
        twice.add(&remainder);
        let up = twice >= power;
        let distance = if up {
            twice.subtract(&power);
            twice
        } else {
            let mut gap = power.clone();
            gap.subtract(&twice);
            gap
        };
        let near = !distance.is_zero()
            && distance.times(&Natural::from_u64(1u64 << (NEAR_TIE_BITS - 1))) < exact.coefficient;
        let (nearest, other) = if up {
            (down.up(), down)
        } else {
            (down, down.up())
        };
        Some(Roundings {
            nearest,
            other: near.then_some(other),
        })
    }

    /// Whether no rounding of `self` is a rounding of `other`.
    fn disjoint(&self, other: &Roundings) -> bool {
        let of = |r: &Roundings| [Some(r.nearest), r.other];
        of(self)
            .iter()
            .flatten()
            .all(|x| !of(other).iter().flatten().any(|y| x == y))
    }
}

/// The order of two texts as the comparison operators give it ([`texts`] for an order), or `None`
/// when the collation does not decide them: calc-func's public `text_order`.
pub(crate) fn text_order(a: &[u16], b: &[u16]) -> Option<Ordering> {
    texts(a, b, Want::Order).ok()
}

/// A key under which texts the comparison operators hold equal always fall together, for a
/// grouping that compares only the texts sharing a key (UNIQUE): the weights the second pass
/// compares but the diacritics (texts the first pass holds equal differ in ASCII case alone,
/// which the weights ignore), or `None` when the collation builds none for `text`.
pub(super) fn text_bucket(text: &[u16]) -> Option<Vec<u8>> {
    if text.iter().any(|&code| sort_key::refused(code)) {
        return None;
    }
    let parts = sort_key::parts(text).ok()?;
    let mut key = parts.primary;
    key.push(1);
    key.extend_from_slice(&parts.rest);
    Some(key)
}

/// The order of two texts as the comparison operators give it, or [`Beyond`] when either holds a
/// character beyond the collation's repertoire and they are not the same. For [`Want::Equality`]
/// any unequal pair is `Less`.
pub(super) fn texts(a: &[u16], b: &[u16], want: Want) -> Result<Ordering, Beyond> {
    if a == b {
        return Ok(Ordering::Equal);
    }
    let ordering = collation::compare(a, b)?;
    Ok(match want {
        Want::Equality if ordering.is_ne() => Ordering::Less,
        Want::Equality | Want::Order => ordering,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rounded(digits: u64, exponent: i32) -> Rounded {
        Rounded { digits, exponent }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn roundings_follow_the_exact_decimal_value() {
        let of = |x: f64| Roundings::of(x).expect("a modelled magnitude");
        let one = of(1.0);
        assert_eq!(
            (one.nearest, one.other),
            (rounded(100_000_000_000_000, 0), None)
        );
        // 0.1 + 0.2 is 0.3000000000000000444..., 0.3 is 0.2999999999999999888...
        assert_eq!(of(0.1 + 0.2).nearest, of(0.3).nearest);
        // 999999999999999.5 is an exact tie: away from zero, carrying into a new digit.
        let tie = of(999_999_999_999_999.5);
        assert_eq!(
            (tie.nearest, tie.other),
            (rounded(100_000_000_000_000, 15), None)
        );
        assert!(Roundings::of(f64::MAX).is_none());
        assert!(Roundings::of(1.0001e-146).is_none());
        assert!(Roundings::of(1.01e-146).is_some());
    }
}

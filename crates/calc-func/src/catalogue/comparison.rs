//! The comparison family fragment: the comparison operators and their kernels.
//!
//! A comparison coerces neither operand. An operand's error is the result, the left operand's
//! first. Values of different kinds order as numbers, then text, then logicals (FALSE before
//! TRUE). A blank operand (Empty, or Missing) compares as the empty value of the other operand's
//! kind (0, empty text or FALSE), and two blanks are equal.
//!
//! Two numbers order by value. Equal numbers are equal without any rounding. Two different numbers
//! are first tested for near-equality, which is an arithmetic site: it rounds a difference and a
//! product under the call's numeric mode, so under Exact, which has no implementation, the
//! comparison is refused with ExactUnavailable. Under Fast two different numbers within a relative
//! 1e-13 of each other are refused: Excel compares some nearly equal numbers as equal, by a rule
//! that is not modelled.
//!
//! Text compares ignoring case: texts with the same code units are equal, two ASCII texts are
//! equal when they are equal ignoring ASCII case, and two texts of ASCII letters and digits order
//! with digits before letters and a prefix first. Any other text comparison needs Excel's
//! collation, which is not modelled, and is refused.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};
use crate::call::{EvalServices, FunctionFault};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CoreValue};
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

/// The relative distance within which two different numbers are refused, because Excel may
/// compare them as equal. Excel's rule for nearly equal numbers, and with it the true width of this
/// band, belongs to cdoc-jpha, which replaces the refusal with the rule the capture shows.
const NEAR: f64 = 1e-13;

/// What a comparison asks of two texts: whether they are equal, or how they order.
#[derive(Clone, Copy)]
enum Want {
    Equality,
    Order,
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
    Text(&'a [u16]),
    Logical(bool),
    Blank,
}

impl Operand<'_> {
    /// The order of kinds: numbers, then text, then logicals.
    fn kind(self) -> u8 {
        match self {
            Operand::Number(_) | Operand::Blank => 0,
            Operand::Text(_) => 1,
            Operand::Logical(_) => 2,
        }
    }

    /// The empty value of this operand's kind, which a blank compares as.
    fn blank(self) -> Self {
        match self {
            Operand::Number(_) => Operand::Number(0.0),
            Operand::Text(_) => Operand::Text(&[]),
            Operand::Logical(_) => Operand::Logical(false),
            Operand::Blank => Operand::Blank,
        }
    }
}

fn order(site: &Site<'_>, arguments: &[CalcValue], want: Want) -> Step<Ordering> {
    site.scalar_operands(arguments)?;
    let mut operands = [Operand::Blank; 2];
    for (index, argument) in arguments.iter().enumerate() {
        operands[index] = match argument.core() {
            CoreValue::Number(n) => Operand::Number(site.argument(index, coerce::operand(*n))?),
            CoreValue::Text(text) => Operand::Text(text.utf16_code_units()),
            CoreValue::Logical(b) => Operand::Logical(*b),
            CoreValue::Empty | CoreValue::Missing => Operand::Blank,
            CoreValue::Error(code) => return Err(Exit::Error(*code)),
            CoreValue::Array(_) | CoreValue::Reference(_) => {
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
        (Operand::Text(a), Operand::Text(b)) => {
            texts(a, b, want).ok_or_else(|| site.not_modelled(None))
        }
        (Operand::Logical(a), Operand::Logical(b)) => Ok(a.cmp(&b)),
        (a, b) => Ok(a.kind().cmp(&b.kind())),
    }
}

/// The order of two finite numbers. Equal numbers need no rounding. For different numbers the
/// near-equality test computes their difference and the band [`NEAR`] around the larger magnitude
/// at the call's arithmetic sites, which round under its numeric mode; a pair within the band is
/// refused as not modelled.
fn numbers(site: &Site<'_>, a: f64, b: f64) -> Step<Ordering> {
    if a == b {
        return Ok(Ordering::Equal);
    }
    let difference = site.subtract(a, b)?.abs();
    let band = site.multiply(a.abs().max(b.abs()), NEAR)?;
    if difference <= band {
        return Err(site.not_modelled(None));
    }
    Ok(a.total_cmp(&b))
}

/// The order of two texts, or `None` when it needs Excel's collation. For [`Want::Equality`] any
/// unequal pair is `Less`.
fn texts(a: &[u16], b: &[u16], want: Want) -> Option<Ordering> {
    if a == b {
        return Some(Ordering::Equal);
    }
    match want {
        Want::Equality => {
            let ascii = |text: &[u16]| text.iter().all(|&unit| unit < 0x80);
            (ascii(a) && ascii(b)).then(|| {
                let folded = |text: &[u16]| {
                    text.iter()
                        .map(|&unit| (unit as u8).to_ascii_uppercase())
                        .collect::<Vec<u8>>()
                };
                if folded(a) == folded(b) {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }
            })
        }
        Want::Order => {
            let ranks = |text: &[u16]| -> Option<Vec<u8>> {
                text.iter()
                    .map(|&unit| match u8::try_from(unit).ok()? {
                        digit @ b'0'..=b'9' => Some(digit - b'0'),
                        letter if letter.is_ascii_alphabetic() => {
                            Some(10 + (letter.to_ascii_uppercase() - b'A'))
                        }
                        _ => None,
                    })
                    .collect()
            };
            Some(ranks(a)?.cmp(&ranks(b)?))
        }
    }
}

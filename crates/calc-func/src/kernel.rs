//! What the kernels share: how a kernel stops (with a worksheet error, which is a computed result,
//! or with a fault, which is not), the arithmetic sites, which round under the call's
//! [`NumericMode`], and the rule for publishing a number result.

use crate::call::{EvalServices, FunctionFault};
use crate::catalogue::FunctionId;
use crate::coerce::{self, Numeric, Stop};
use crate::decimal::ExactDecimal;
use crate::locale::CalcEnvironment;
use crate::value_text;
use crate::value::{CalcValue, CoreValue, NumericMode, WorksheetErrorCode};

/// Why a kernel stopped before its value.
#[derive(Debug)]
pub(crate) enum Exit {
    /// The call's result is this worksheet error.
    Error(WorksheetErrorCode),
    /// The call ends in this fault.
    Fault(FunctionFault),
}

/// A kernel's step: a value, or the reason it stopped.
pub(crate) type Step<T> = Result<T, Exit>;

/// One call's kernel context: the entry's ID, which its faults name, and the numeric mode its
/// arithmetic sites round under.
pub(crate) struct Site<'m> {
    id: FunctionId,
    mode: &'m NumericMode,
    environment: Option<CalcEnvironment>,
}

impl<'m> Site<'m> {
    pub(crate) fn new(id: FunctionId, services: &EvalServices<'m>) -> Self {
        Site {
            id,
            mode: services.numeric_mode(),
            environment: services.environment(),
        }
    }

    pub(crate) fn id(&self) -> FunctionId {
        self.id
    }

    /// The call's environment, when the caller supplied one.
    pub(crate) fn environment(&self) -> Option<&CalcEnvironment> {
        self.environment.as_ref()
    }

    /// The call's result from a kernel's last step: a worksheet error that stopped it is the
    /// value it computed.
    pub(crate) fn finish(step: Step<CalcValue>) -> Result<CalcValue, FunctionFault> {
        match step {
            Ok(value) => Ok(value),
            Err(Exit::Error(code)) => Ok(CalcValue::error(code)),
            Err(Exit::Fault(fault)) => Err(fault),
        }
    }

    /// The refusal of a call whose Excel result is not modelled; `argument` names the argument
    /// responsible when one is.
    pub(crate) fn not_modelled(&self, argument: Option<usize>) -> Exit {
        Exit::Fault(FunctionFault::NotImplemented {
            id: self.id,
            argument,
        })
    }

    /// A coercion's stop, attributed to `argument`.
    pub(crate) fn stop(&self, stop: Stop, argument: Option<usize>) -> Exit {
        match stop {
            Stop::Error(code) => Exit::Error(code),
            Stop::NotModelled => self.not_modelled(argument),
        }
    }

    /// Argument `index` as a coercion read it.
    pub(crate) fn argument<T>(&self, index: usize, coerced: Result<T, Stop>) -> Step<T> {
        coerced.map_err(|stop| self.stop(stop, Some(index)))
    }

    /// Argument `index` read through worksheet number coercion. Text that reads as a number is
    /// converted to its nearest binary64 here, an arithmetic site: the conversion rounds.
    ///
    /// Text that coercion's plain decimal form does not decide (a date, a time, a fraction, a
    /// currency amount, grouped digits, a percentage, more than 15 significant digits) is read as
    /// VALUE reads it ([`crate::value_text`]) when the call's environment holds a locale profile,
    /// and refused as not modelled when it does not.
    pub(crate) fn number_argument(&self, index: usize, value: &CalcValue) -> Step<f64> {
        let numeric = match coerce::number(value) {
            Err(Stop::NotModelled) => {
                return match self.read_text(index, value)? {
                    Some(n) => Ok(n),
                    None => Err(self.not_modelled(Some(index))),
                };
            }
            other => self.argument(index, other)?,
        };
        match numeric {
            Numeric::Number(n) => Ok(n),
            Numeric::Decimal(decimal) => {
                self.rounding()?;
                self.argument(index, decimal.nearest())
            }
        }
    }

    /// Text read as VALUE reads it under the call's locale profile: `Some` number, #VALUE! when
    /// the grammar does not read it, `None` when the call has no profile, and the refusal of text
    /// whose reading is not modelled. Reading rounds once to binary64, an arithmetic site.
    pub(crate) fn read_text(&self, index: usize, value: &CalcValue) -> Step<Option<f64>> {
        let CoreValue::Text(text) = value.core() else {
            return Ok(None);
        };
        let Some(environment) = self.environment.as_ref() else {
            return Ok(None);
        };
        let Some(profile) = environment.profile.as_ref() else {
            return Ok(None);
        };
        self.rounding()?;
        let context = value_text::ReadContext {
            profile,
            date_system: environment.date_system,
            now: environment.now,
        };
        match value_text::read(text.utf16_code_units(), &context) {
            Ok(reading) => Ok(Some(reading.value())),
            Err(value_text::Unread::Rejected) => Err(Exit::Error(WorksheetErrorCode::Value)),
            Err(value_text::Unread::NotModelled) => Err(self.not_modelled(Some(index))),
        }
    }

    /// Refuses the first array operand of an operator, or the first array argument of a function
    /// whose parameters take single values: either applied to an array lifts over its elements,
    /// which is not modelled. This comes before any operand's error, since a lifted result is an
    /// array even when an operand is an error.
    pub(crate) fn scalar_operands(&self, operands: &[CalcValue]) -> Step<()> {
        match operands
            .iter()
            .position(|operand| matches!(operand.core(), CoreValue::Array(_)))
        {
            Some(index) => Err(self.not_modelled(Some(index))),
            None => Ok(()),
        }
    }

    /// Every arithmetic site rounds once under the call's mode. Fast is IEEE 754 binary64 with
    /// round-to-nearest-even; Exact has no implementation, so a site under it is refused.
    fn rounding(&self) -> Step<()> {
        match self.mode {
            NumericMode::Fast => Ok(()),
            NumericMode::Exact(_) => {
                Err(Exit::Fault(FunctionFault::ExactUnavailable { id: self.id }))
            }
        }
    }

    /// The arithmetic site of a kernel that computes in binary64 as a whole (the ported numeric
    /// kernels): it rounds under Fast and is refused under Exact, like every other site.
    pub(crate) fn binary64(&self) -> Step<()> {
        self.rounding()
    }

    pub(crate) fn add(&self, a: f64, b: f64) -> Step<f64> {
        self.rounding()?;
        Ok(a + b)
    }

    pub(crate) fn subtract(&self, a: f64, b: f64) -> Step<f64> {
        self.rounding()?;
        Ok(a - b)
    }

    pub(crate) fn multiply(&self, a: f64, b: f64) -> Step<f64> {
        self.rounding()?;
        Ok(a * b)
    }

    /// The binary64 nearest a decimal that a kernel computed exactly. The conversion rounds once;
    /// a decimal beyond binary64's range converts to an infinity, which the number rule publishes
    /// as #NUM!. A decimal that is not zero but lies at or below half the smallest subnormal
    /// would round to a zero: it is refused, as a subnormal result is, because Excel's handling of
    /// a result below the normal range is not modelled.
    pub(crate) fn nearest(&self, decimal: &ExactDecimal) -> Step<f64> {
        self.rounding()?;
        let nearest: f64 = decimal
            .text()
            .parse()
            .expect("an exact decimal reads as a binary64");
        if nearest == 0.0 && !decimal.coefficient.is_zero() {
            return Err(self.not_modelled(None));
        }
        Ok(nearest)
    }

    /// `a / b`; #DIV/0! when `b` is 0, which needs no rounding.
    pub(crate) fn divide(&self, a: f64, b: f64) -> Step<f64> {
        if b == 0.0 {
            return Err(Exit::Error(WorksheetErrorCode::Div0));
        }
        self.rounding()?;
        Ok(a / b)
    }

    /// A number result as the worksheet holds it. A result that is not finite (an overflow) is
    /// #NUM!. Zero is +0 whatever its sign: a worksheet number carries no negative zero. A
    /// subnormal result is refused, because Excel's handling of one is not modelled.
    pub(crate) fn number(&self, x: f64) -> Step<CalcValue> {
        if !x.is_finite() {
            Err(Exit::Error(WorksheetErrorCode::Num))
        } else if x == 0.0 {
            Ok(CalcValue::number(0.0))
        } else if x.is_subnormal() {
            Err(self.not_modelled(None))
        } else {
            Ok(CalcValue::number(x))
        }
    }
}

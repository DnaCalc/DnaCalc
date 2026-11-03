//! LAMBDA values: the callable a LAMBDA expression evaluates to.
//!
//! A LAMBDA's body is a block of the plan it was compiled into, and its value carries what a call
//! of it needs: the block, its parameters and the locals it captured when it was evaluated (the
//! LET names and outer LAMBDA parameters its body can see; lexical, not dynamic, scope). The value
//! is a calc-value [`CallableValue`] whose handle is a [`Closure`], so it flows through calc-func
//! like any other value; its worksheet face is #CALC!, as Excel shows a LAMBDA in a cell. Only the
//! evaluation of the plan that made it calls it: the closure names its plan, and a call of a
//! closure of another plan is refused.
//!
//! Ported from OxFml's `eval/mod.rs` (`LambdaBinding`, `CallableRegistry`,
//! `helper_closure_from_names`), which registered closures by a text token in a per-evaluation
//! registry; here the closure is the handle itself.

use calc_func::value::{CalcValue, CallableArityShape, CallableValue, OpaqueCallable};
use std::any::Any;
use std::sync::Arc;

/// A LAMBDA value's identity and environment.
#[derive(Debug)]
pub(super) struct Closure {
    /// Own the body even after its originating instance and interner disappear.
    pub(super) plan: Arc<super::CompiledPlan>,
    pub(super) block: usize,
    pub(super) params: usize,
    pub(super) required: usize,
    pub(super) captured: Box<[CalcValue]>,
}

impl OpaqueCallable for Closure {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Closure {
    /// The closure as a value.
    pub(super) fn into_value(self) -> CalcValue {
        let arity = CallableArityShape::range(self.required, self.params);
        CalcValue::callable(CallableValue::new(arity, "LAMBDA", Arc::new(self)))
    }

    /// The closure behind `value`, when it is one.
    pub(super) fn of(value: &CalcValue) -> Option<&Closure> {
        value
            .callable_value()
            .and_then(|callable| callable.handle().as_any().downcast_ref::<Closure>())
    }
}

/// A function value (dc-h9u): a catalogue function named where a LAMBDA value is taken, which a
/// call of it calls eagerly with the arguments it is given. Its worksheet face is #CALC!, as a
/// LAMBDA's is.
#[derive(Debug)]
pub(super) struct FunctionValue {
    pub(super) target: super::Target,
}

impl OpaqueCallable for FunctionValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl FunctionValue {
    /// The function as a value, with the arity of its catalogue entry.
    pub(super) fn into_value(self) -> CalcValue {
        let arity = self.target.0.arity();
        let spelling = self.target.0.spelling().to_owned();
        CalcValue::callable(CallableValue::new(arity, &spelling, Arc::new(self)))
    }

    /// The function value behind `value`, when it is one.
    pub(super) fn of(value: &CalcValue) -> Option<&FunctionValue> {
        value
            .callable_value()
            .and_then(|callable| callable.handle().as_any().downcast_ref::<FunctionValue>())
    }
}

//! Engine-neutral user-function traits. CalcFml owns callable registration and binding.

use crate::value::{CalcValue, CellKey, ReferenceLike};
use std::{any::Any, fmt, rc::Rc};

/// Neutral callable identity; CalcFml alone allocates tables, slots and generations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserCallableIdentity {
    pub table: u64,
    pub slot: u32,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserInputKey {
    pub callable: UserCallableIdentity,
    pub subscription: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UserInputRead {
    pub key: UserInputKey,
    pub sequence: u64,
}
/// One immutable external input cut, fixed at command admission. RTD never completes a call.
pub trait UserExternalInputs: fmt::Debug + Send + Sync {
    fn version(&self) -> u64;
    fn rtd_value(&self, key: UserInputKey) -> Option<(u64, CalcValue)>;
}

/// Host-owned identity and immutable read-view lease; no borrowed evaluator pointers.
#[derive(Clone)]
pub struct UserInvocationFrame {
    pub id: u64,
    pub parent: Option<u64>,
    pub read_view: Rc<dyn Any>,
    /// Formula assignment evaluates on its owner, including thread-safe native UDFs.
    pub formula_entry: bool,
}
impl UserInvocationFrame {
    pub fn new(parent: Option<u64>, read_view: Rc<dyn Any>) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("invocation identity exhausted");
        Self {
            id,
            parent,
            read_view,
            formula_entry: false,
        }
    }
    pub fn with_formula_entry(mut self, entry: bool) -> Self {
        self.formula_entry = entry;
        self
    }
}
impl fmt::Debug for UserInvocationFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserInvocationFrame")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("formula_entry", &self.formula_entry)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum UserFunctionStep {
    Suspended,
    /// Park this activation until the owner delivers this request through its checked token.
    Awaiting {
        request: u64,
    },
    Complete(CalcValue),
}
/// Owning activation. Resume never re-enters the procedure or repeats a completed effect.
pub trait UserFunctionInvocation: fmt::Debug {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        quantum: u64,
    ) -> Result<UserFunctionStep, String>;
}

/// An owned reference identity. Tracked handles never identify a workbook by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StoredReference {
    Inline(ReferenceLike),
    Tracked { owner: u64, slot: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceHandleError {
    ForeignWorkbook,
    ClosedWorkbook,
    Deleted,
    ExpiredView,
    NotModelledEdit,
}
impl fmt::Display for ReferenceHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
pub trait ReferenceHandles {
    fn retain(&self, reference: &ReferenceLike) -> Result<StoredReference, ReferenceHandleError>;
    fn resolve(&self, reference: &StoredReference) -> Result<ReferenceLike, ReferenceHandleError>;
}

/// A registration's permission to end one semantic evaluation and request a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UserReevaluationReason {
    XllUncalcedUnwind,
    ProvisionalArguments,
}

/// A user function. The default host entry reads references eagerly; adapters can instead
/// use live reference/caller services for one invocation. An adapter
/// returns worksheet errors as values and execution failures as diagnostics (mapped to #VALUE!).
/// No Send/Sync promise is made: a desktop add-in may belong to its owning thread.
pub trait UserFunction: fmt::Debug {
    /// Maximum independent scalar-cell activations allowed together. This is
    /// fixed when registering the callable; adapters opt in explicitly.
    fn max_parallel_calls(&self) -> usize {
        1
    }
    fn reevaluation_policy(&self) -> Option<UserReevaluationReason> {
        None
    }
    /// Adapters sharing project state return the same lane while their leases are alive.
    fn state_lane(&self) -> Option<u64> {
        None
    }
    fn permits_reentry(&self) -> bool {
        false
    }
    fn cached_reads(&self) -> bool {
        false
    }
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String>;

    /// Prepare a cooperative activation when this exact signature/body supports it. None is
    /// a declared synchronous interval, chosen before entry; errors never fall back or replay.
    fn begin_call(
        &self,
        _arguments: &[CalcValue],
        _frame: UserInvocationFrame,
        _context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
        Ok(None)
    }

    /// Reference-aware host entry. Eager adapters keep the original value-call behavior.
    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let arguments = arguments
            .iter()
            .map(|value| match value.as_reference() {
                Some(reference) => context.argument_values_of(reference),
                None => Ok(value.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.call(&arguments)
    }
}

/// Services borrowed only for one synchronous call. The evaluator owns the reader and caller;
/// adapters can retain reference identities, and must read their values through this context.
pub trait UserFunctionContext {
    fn caller(&self) -> Option<CellKey>;
    /// Exact provider generation admitted for this invocation, after table replacement.
    fn callable_identity(&self) -> Option<UserCallableIdentity> {
        None
    }
    fn rtd_value(&mut self, _subscription: u64) -> Result<Option<(u64, CalcValue)>, String> {
        Err(self.refuse_by("RTD input without an admitted input view", "dc-hsa5.4"))
    }
    /// End this semantic evaluation after an unresolved read. The registration authorizes the reason.
    fn request_reevaluation(&mut self, _reason: UserReevaluationReason) -> Result<(), String> {
        Err(self.refuse_by("semantic reevaluation without an owner", "dc-hsa5.4"))
    }
    /// Invocation-aware callback; never queue it behind the caller's waiting activation.
    fn call_user_function(
        &mut self,
        _name: &str,
        _arguments: &[CalcValue],
    ) -> Result<CalcValue, String> {
        Err(self.refuse_by(
            "nested user callbacks without an invocation frame",
            "dc-hsa5.4",
        ))
    }
    /// Consume the return supplied to this activation. It is unavailable in another frame.
    fn take_async_result(&mut self, _request: u64) -> Option<CalcValue> {
        None
    }
    fn retain_reference(
        &mut self,
        reference: &ReferenceLike,
    ) -> Result<StoredReference, ReferenceHandleError> {
        Ok(StoredReference::Inline(reference.clone()))
    }
    fn resolve_reference(
        &mut self,
        reference: &StoredReference,
    ) -> Result<ReferenceLike, ReferenceHandleError> {
        match reference {
            StoredReference::Inline(reference) => Ok(reference.clone()),
            _ => Err(ReferenceHandleError::ForeignWorkbook),
        }
    }
    fn invocation(&self) -> Option<&UserInvocationFrame> {
        None
    }
    /// Set this formula cell's volatility for the current evaluation. The last executed
    /// request wins; a host that only performs explicit calls may ignore scheduling effects.
    fn set_volatile(&mut self, _volatile: bool) {}
    /// Record a host capability refusal. It must survive VBA error handlers and formula IFERROR.
    fn refuse(&mut self, reason: &'static str) -> String;
    /// Record the adapter that owns an absent capability. Older adapters keep their default
    /// owner through `refuse`; native adapters can identify their own work package.
    fn refuse_by(&mut self, reason: &'static str, _owner: &'static str) -> String {
        self.refuse(reason)
    }
    fn values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String>;
    /// Explicit scalar-argument readiness is a dependency, distinct from a VBA Range read.
    fn argument_values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String> {
        self.values_of(reference)
    }
    /// Invoke the ordinary CalcFunc catalogue through the evaluator's existing call route.
    fn worksheet_function(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, String>;
    /// The same worksheet callback, retaining the native adapter that owns an absent service.
    fn worksheet_function_for(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
        _owner: &'static str,
    ) -> Result<CalcValue, String> {
        self.worksheet_function(name, arguments)
    }
    /// Compile and run a constant worksheet expression through the owning formula route.
    /// References, defined names, nested user calls and volatility require a broader contract.
    fn evaluate_constant_expression(
        &mut self,
        _expression: &str,
        owner: &'static str,
    ) -> Result<CalcValue, String> {
        Err(self.refuse_by("constant expression evaluation without an owner", owner))
    }
}

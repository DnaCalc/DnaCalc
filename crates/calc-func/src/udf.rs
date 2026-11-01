//! Engine-neutral user-function traits. CalcFml owns callable registration and binding.

use crate::value::{CalcValue, CellKey, ReferenceLike};
use std::{any::Any, fmt, rc::Rc};

/// Host-owned identity and immutable read-view lease; no borrowed evaluator pointers.
#[derive(Clone)]
pub struct UserInvocationFrame {
    pub id: u64,
    pub parent: Option<u64>,
    pub read_view: Rc<dyn Any>,
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
        }
    }
}
impl fmt::Debug for UserInvocationFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserInvocationFrame")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum UserFunctionStep {
    Suspended,
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

/// A user function. The default host entry reads references eagerly; adapters can instead
/// use live reference/caller services for one invocation. An adapter
/// returns worksheet errors as values and execution failures as diagnostics (mapped to #VALUE!).
/// No Send/Sync promise is made: a desktop add-in may belong to its owning thread.
pub trait UserFunction: fmt::Debug {
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
}

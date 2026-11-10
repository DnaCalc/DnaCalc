//! Engine-neutral user-function traits. CalcFml owns callable registration and binding.

use crate::value::{CalcValue, CellKey, ReferenceLike};
use std::{any::Any, fmt, rc::Rc};

/// Provider key for the host-owned built-in real-time-data route.
pub use calc_catalogue::unheld::RTD_PROVIDER;

/// Neutral callable identity; CalcFml alone allocates tables, slots and generations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserCallableIdentity {
    pub table: u64,
    pub slot: u32,
    pub generation: u64,
}
/// Excel's identity of one real-time-data topic: the server's ProgID, the (local) server name and
/// the topic strings, 1 to 253 of them. Every cell that asks for the same arguments shares one
/// topic. Measured in Excel 16.0 (2026-11-08, dc-snz.23.4, codex-runs/rtd1/measure): the ProgID
/// compares without regard to case; an omitted server and an empty one are one; topic strings
/// compare exactly, case and spaces included, after text coercion (the number 1 and the text
/// "1", TRUE and "TRUE", `0.1+0.2` and "0.3" are one topic); two strings and one are two topics;
/// a typed `=RTD()` and an XLL's `xlfRtd` with equal arguments are one topic. The ProgID's case
/// is folded for ASCII only; the measurement did not reach other letters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicKey {
    pub progid: String,
    pub server: String,
    pub strings: Vec<String>,
}
impl TopicKey {
    pub fn new(progid: &str, server: &str, strings: &[String]) -> Self {
        TopicKey {
            progid: progid.to_ascii_lowercase(),
            server: server.to_owned(),
            strings: strings.to_vec(),
        }
    }
}
/// One connection incarnation of a topic: a new lease each time the topic is connected anew. A
/// lease is never reused, so a value of an old incarnation cannot reach a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RtdLease(pub u64);
/// The admitted state of one topic: its lease, the sequence of its latest value and the value.
#[derive(Clone, Debug, PartialEq)]
pub struct RtdEntry {
    pub lease: RtdLease,
    pub sequence: u64,
    pub value: CalcValue,
}
/// What a result read of a topic: the lease the reader asked for and the sequence it saw in its
/// cut (0 when the cut held nothing for that lease).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RtdRead {
    pub topic: TopicKey,
    pub lease: RtdLease,
    pub sequence: u64,
}
/// The prefix of the error text when the owner's connector could not connect a topic (no server
/// for the ProgID, or its start failed). The built-in `RTD` answers it as Excel does, `#N/A`.
pub const RTD_CONNECT_FAILED: &str = "dc-snz.23: RTD connect failed: ";
/// What the owner's connector answers for one topic: the connection's lease and, while the
/// owner has not yet admitted it, the value ConnectData returned (its sequence is 1 for a new
/// connection). The provisional value is shown at once, as Excel shows it, but it is not
/// certified: the owner admits it as that sequence before the result is published.
#[derive(Clone, Debug, PartialEq)]
pub struct RtdConnection {
    pub lease: RtdLease,
    pub provisional: Option<(u64, CalcValue)>,
}
/// The owner's capability to join or start a topic's connection, supplied for one evaluation as
/// a reference reader is. Calling it again for a topic already connected joins it (no second
/// ConnectData) and registers `caller` as one more cell that reads the topic. Absent in the
/// browser, where an RTD read is a typed refusal.
///
/// `declared` says who keeps the connection alive. A formula that reads the topic itself (the
/// built-in `RTD`) declares it for as long as the formula stays; an add-in's `xlfRtd` call does
/// not, since the native call's own record of what it asked for each time decides.
pub trait RtdConnector: fmt::Debug {
    fn connect(
        &self,
        topic: &TopicKey,
        caller: CellKey,
        declared: bool,
    ) -> Result<RtdConnection, String>;
}
/// What the one read-or-connect rule gives a reader: the lease it read at and the value with the
/// sequence it was admitted (or provisionally observed) at, if there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct RtdTopicRead {
    pub lease: RtdLease,
    pub value: Option<(u64, CalcValue)>,
}
/// One immutable external input cut, fixed at command admission, keyed by topic identity. RTD
/// never completes a call. The cut is readable from any thread.
pub trait RtdCut: fmt::Debug + Send + Sync {
    fn version(&self) -> u64;
    fn get(&self, topic: &TopicKey) -> Option<RtdEntry>;
}

/// Host-owned identity and immutable read-view lease; no borrowed evaluator pointers.
#[derive(Clone)]
pub struct UserInvocationFrame {
    pub id: u64,
    pub parent: Option<u64>,
    pub read_view: Rc<dyn Any>,
    /// Formula assignment evaluates on its owner, including thread-safe native UDFs.
    pub formula_entry: bool,
    /// A single-threaded calculation (Excel's `Range.Calculate`): thread-safe native UDFs are
    /// entered on the owner too. Excel 16.0.20527 entered every `QU$` call on its main thread
    /// there, the status 64 attempt and its retry alike (x17).
    pub single_threaded: bool,
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
            single_threaded: false,
        }
    }
    pub fn with_formula_entry(mut self, entry: bool) -> Self {
        self.formula_entry = entry;
        self
    }
    pub fn with_single_threaded(mut self, single: bool) -> Self {
        self.single_threaded = single;
        self
    }
    /// Whether native calls of this invocation stay on the owner thread.
    pub fn owner_only(&self) -> bool {
        self.formula_entry || self.single_threaded
    }
}
impl fmt::Debug for UserInvocationFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserInvocationFrame")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("formula_entry", &self.formula_entry)
            .field("single_threaded", &self.single_threaded)
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

/// What a typed reference read gives an adapter: the value, or `Pending` when the reference is
/// valid but a producer it covers has not been calculated yet in this command. Only a
/// registration whose policy is [`UserReevaluationReason::XllUncalcedUnwind`] is told `Pending`
/// (Excel's status 64, `xlretUncalced`); every other reader gets the value or a fault as before.
/// A `Pending` read records no refusal: the adapter returns, the evaluation ends, and the engine
/// evaluates the formula again after the producer.
#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceRead {
    Value(CalcValue),
    Pending,
}

/// A user function. The default host entry reads references eagerly; adapters can instead
/// use live reference/caller services for one invocation. An adapter
/// returns worksheet errors as values and execution failures as diagnostics (mapped to #VALUE!).
/// No Send/Sync promise is made: a desktop add-in may belong to its owning thread.
pub trait UserFunction: fmt::Debug {
    /// Owner-granted permission to run an argument-ready call before cached-read
    /// observers in the measured Full/Rebuild profile. This is independent of
    /// thread safety, memoization and permission to replay an invocation.
    fn permits_early_execution(&self) -> bool {
        false
    }
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

/// The sheet and the selection a person has active in the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserView {
    pub active_sheet: crate::value::SheetId,
    /// The selected cell (a host with a one-cell selection; a larger one is not modelled).
    pub selection: CellKey,
}

/// Services borrowed only for one synchronous call. The evaluator owns the reader and caller;
/// adapters can retain reference identities, and must read their values through this context.
pub trait UserFunctionContext {
    fn caller(&self) -> Option<CellKey>;
    /// What the person looking at the application has active: the host's answer for VBA's
    /// `ActiveSheet` and `Selection`. In Excel a worksheet function sees the user's sheet and
    /// selection, not its caller's (measured, dc-0f1k.12). `None` when the host does not model
    /// that view: the calls are then typed refusals.
    fn user_view(&self) -> Option<UserView> {
        None
    }
    /// The plain name of a sheet (for `ActiveSheet.Name`), from the same metadata view as
    /// [`UserFunctionContext::reference_qualifier`].
    fn sheet_name(&mut self, _sheet: crate::value::SheetId) -> Result<String, String> {
        Err(self.refuse("sheet names without an admitted metadata view"))
    }
    /// The current workbook/sheet qualifier of a local reference, without `!`.
    /// Reads metadata only; no formula value is demanded or observed.
    fn reference_qualifier(&mut self, _sheet: crate::value::SheetId) -> Result<String, String> {
        Err(self.refuse("reference names without an admitted metadata view"))
    }
    /// Exact provider generation admitted for this invocation, after table replacement.
    fn callable_identity(&self) -> Option<UserCallableIdentity> {
        None
    }
    /// The cut's value for `topic` at `lease`, recording the read. `None` when the cut holds
    /// nothing for that lease.
    fn rtd_value(
        &mut self,
        _topic: &TopicKey,
        _lease: RtdLease,
    ) -> Result<Option<RtdEntry>, String> {
        Err(self.refuse_by("RTD input without an admitted input view", "dc-hsa5.4"))
    }
    /// The one rule for reading a topic from a function: join or start its connection through
    /// the owner's connector, and take the cut's admitted value, or the connection's
    /// provisional one while it awaits admission. The read is recorded with the sequence used.
    fn read_topic(&mut self, _topic: &TopicKey, _declared: bool) -> Result<RtdTopicRead, String> {
        Err(self.refuse_by("RTD read without an owner connector", "dc-snz.23"))
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
    /// A read from inside the call (an XLL's xlCoerce) that tells a pending producer apart from
    /// a value or a fault. The default never reports `Pending`.
    fn read_reference(&mut self, reference: &ReferenceLike) -> Result<ReferenceRead, String> {
        self.values_of(reference).map(ReferenceRead::Value)
    }
    /// The same for preparing a value argument before entry: `Pending` means the call must not
    /// be entered in this evaluation. The default never reports `Pending`.
    fn read_argument(&mut self, reference: &ReferenceLike) -> Result<ReferenceRead, String> {
        self.argument_values_of(reference).map(ReferenceRead::Value)
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

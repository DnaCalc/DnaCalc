//! Document event sources backed by `calc_object_model::ObjectModel` (dc-piw0.5).
//!
//! Each `EventSource` wraps an `ObjectRef` (`_Worksheet` or `_Workbook`) as a service-backed
//! `calcvba_host_api::HostObject` using the real descriptor IID and an `EventHub` validating
//! real Excel `DocEvents` / `WorkbookEvents` DISPIDs.
use crate::{invocation_model, projection};
use calc_func::value::{CalcValue, ReferenceLike};
use calc_object_model::{AutoError, ObjectModel, ObjectRef};
use calcvba_host_api as api;
use std::{
    cell::RefCell,
    rc::Rc,
};

/// Borrowed only while a synchronous standalone handler runs (tests and `load_office_project`).
/// Workbook hosts lend their `HostSession` directly through `WorkbookAutomation::with_event_session`.
pub trait EventReadContext {
    fn read_reference(&self, reference: &ReferenceLike) -> Result<CalcValue, String>;
    fn resolve_reference(&self, _text: &str) -> Result<ReferenceLike, String> {
        Err("Range lookup without a mutable application owner is not modelled".into())
    }
    fn write_reference(&self, _reference: &ReferenceLike, _value: CalcValue) -> Result<(), String> {
        Err("Range writes without a mutable application owner is not modelled".into())
    }
    fn calculate(&self, _command: super::CalculationCommand) -> Result<(), String> {
        Err("calculation without a mutable application owner is not modelled".into())
    }
    fn events_enabled(&self) -> Result<bool, String> {
        Err("EnableEvents without a mutable application owner is not modelled".into())
    }
    fn set_events_enabled(&self, _enabled: bool) -> Result<(), String> {
        Err("EnableEvents without a mutable application owner is not modelled".into())
    }
}

scoped_tls_hkt::scoped_thread_local!(static EVENT_SCOPE: for<'a> (
    Option<&'a (dyn EventReadContext + 'a)>, u64, &'a RefCell<Option<String>>
));

thread_local! {
    /// Object-model members the execution policy denied since the last run of a procedure,
    /// whether or not the VBA caller caught the error.
    static OBJECT_DENIALS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

thread_local! {
    static RUN_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub(crate) fn record_object_denial(capability: &str, subject: String) {
    OBJECT_DENIALS.with(|d| d.borrow_mut().push((capability.to_owned(), subject)));
}

pub(crate) fn callback_active() -> bool {
    EVENT_SCOPE.is_set()
}

pub(crate) fn lease_is_active(expected: u64) -> bool {
    EVENT_SCOPE.is_set()
        && crate::runtime::spreadsheet::event_view_active()
        && EVENT_SCOPE.with(|(_, current, _)| current == expected)
}

pub(crate) fn record_event_refusal(reason: String) {
    if EVENT_SCOPE.is_set() {
        EVENT_SCOPE.with(|(_, _, refused)| {
            refused.borrow_mut().get_or_insert(reason);
        });
    }
}

pub(crate) fn with_event_read<T>(
    call: impl FnOnce(&dyn EventReadContext) -> Result<T, String>,
) -> Result<T, AutoError> {
    if !EVENT_SCOPE.is_set() || !crate::runtime::spreadsheet::event_view_active() {
        return Err(AutoError::not_modelled(
            "an application command outside its admitted document event",
            "dc-hsa5.12",
        ));
    }
    EVENT_SCOPE.with(|(context, _, _)| {
        let Some(context) = context else {
            return Err(AutoError::not_modelled(
                "standalone event read context is unavailable",
                "dc-hsa5.12",
            ));
        };
        call(context).map_err(|reason| AutoError::not_modelled(reason, "dc-hsa5.12"))
    })
}

struct EventSpreadsheet;
impl crate::runtime::SpreadsheetContext for EventSpreadsheet {
    fn event_view(&self) -> bool {
        true
    }
    fn refuse(&self, reason: &'static str) -> String {
        record_event_refusal(reason.into());
        reason.into()
    }
    fn caller(&self) -> Option<crate::runtime::Range> {
        self.refuse("Application.Caller in a document event is not modelled");
        None
    }
    fn metadata(
        &self,
        _: crate::runtime::Range,
    ) -> Result<crate::runtime::RangeInfo, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range metadata in a document event is not modelled")
            .into())
    }
    fn cell(
        &self,
        _: crate::runtime::Range,
        _: u32,
        _: u32,
    ) -> Result<crate::runtime::Range, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range cells in a document event is not modelled")
            .into())
    }
    fn area(
        &self,
        _: crate::runtime::Range,
        _: u32,
    ) -> Result<crate::runtime::Range, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range areas in a document event is not modelled")
            .into())
    }
    fn read_range(
        &self,
        _: crate::runtime::Range,
    ) -> Result<crate::runtime::Value, crate::runtime::ContextError> {
        Err(self
            .refuse("legacy Range reads in a document event is not modelled")
            .into())
    }
    fn worksheet_function(
        &self,
        _: &str,
        _: &[crate::runtime::Value],
    ) -> Result<crate::runtime::Value, crate::runtime::ContextError> {
        Err(self
            .refuse("WorksheetFunction callbacks in document events are not modelled")
            .into())
    }
    fn set_volatile(&self, _: bool) -> Result<(), crate::runtime::ContextError> {
        Ok(())
    }
}

pub struct EventSource {
    hub: Rc<projection::EventHub>,
    object: api::HostObject,
    model: Rc<dyn ObjectModel>,
    standalone: Option<Rc<invocation_model::InvocationModel>>,
    name: Option<Rc<RefCell<String>>>,
}

impl EventSource {
    pub fn worksheet(name: &str) -> Self {
        Self::new_standalone("Worksheet", name)
    }

    pub fn workbook(name: &str) -> Self {
        Self::new_standalone("Workbook", name)
    }

    /// Construct a document event source backed directly by a host `ObjectModel`
    /// (`WorkbookAutomation`) for a workbook or worksheet `ObjectRef`.
    pub fn for_object(model: Rc<dyn ObjectModel>, target: ObjectRef) -> Result<Self, String> {
        let hub = projection::EventHub::new(target.interface);
        let object = projection::wrap_object(model.clone(), target, None, Some(hub.clone()));
        Ok(Self {
            hub,
            object,
            model,
            standalone: None,
            name: None,
        })
    }

    fn new_standalone(interface: &str, name: &str) -> Self {
        let name_cell = Rc::new(RefCell::new(name.to_owned()));
        let standalone = Rc::new(invocation_model::InvocationModel::with_document_name(
            false,
            Some(name_cell.clone()),
        ));
        let target = invocation_model::standalone_document_object(interface);
        let hub = projection::EventHub::new(target.interface);
        let model: Rc<dyn ObjectModel> = standalone.clone();
        let object = projection::wrap_object(model.clone(), target, None, Some(hub.clone()));
        Self {
            hub,
            object,
            model,
            standalone: Some(standalone),
            name: Some(name_cell),
        }
    }

    /// Refresh the standalone document identity without replacing its object or subscriptions.
    pub fn set_name(&self, name: &str) {
        if let Some(cell) = &self.name {
            *cell.borrow_mut() = name.to_owned();
        }
    }

    pub(super) fn object(&self) -> &api::HostObject {
        &self.object
    }

    pub fn subscription_count(&self) -> usize {
        self.hub.subscription_count()
    }

    pub fn open(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Open", &mut [], Some(read), max_steps, next_lease())
    }

    /// `Workbook_Open` against the attached host `ObjectModel` (no `EventReadContext` needed).
    pub fn open_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("Open", &mut [], None, max_steps, next_lease())
    }

    /// `Workbook_AddinInstall`: the workbook was installed as an add-in.
    pub fn addin_install(&self, read: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("AddinInstall", &mut [], Some(read), max_steps, next_lease())
    }

    pub fn addin_install_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("AddinInstall", &mut [], None, max_steps, next_lease())
    }

    pub fn change(
        &self,
        target: ReferenceLike,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<(), String> {
        let Some(standalone) = &self.standalone else {
            return Err("change(ReferenceLike) requires a standalone EventSource".into());
        };
        let lease = next_lease();
        let target_ref = standalone.register_event_range(target);
        let target_obj = projection::wrap_object(self.model.clone(), target_ref, Some(lease), None);
        self.raise(
            "Change",
            &mut [api::Value::Object(Some(target_obj))],
            Some(context),
            max_steps,
            lease,
        )
    }

    /// `Worksheet_Change` against the attached host `ObjectModel`, wrapping an already-acquired
    /// `Range` `ObjectRef` under a per-event lease.
    pub fn change_object(&self, target: ObjectRef, max_steps: u64) -> Result<(), String> {
        let lease = next_lease();
        let target_obj = projection::wrap_object(self.model.clone(), target, Some(lease), None);
        self.raise(
            "Change",
            &mut [api::Value::Object(Some(target_obj))],
            None,
            max_steps,
            lease,
        )
    }

    pub fn calculate(&self, context: &dyn EventReadContext, max_steps: u64) -> Result<(), String> {
        self.raise("Calculate", &mut [], Some(context), max_steps, next_lease())
    }

    pub fn calculate_model(&self, max_steps: u64) -> Result<(), String> {
        self.raise("Calculate", &mut [], None, max_steps, next_lease())
    }

    pub fn before_save(
        &self,
        save_as_ui: bool,
        context: &dyn EventReadContext,
        max_steps: u64,
    ) -> Result<bool, String> {
        let mut args = [api::Value::Boolean(save_as_ui), api::Value::Boolean(false)];
        self.raise(
            "BeforeSave",
            &mut args,
            Some(context),
            max_steps,
            next_lease(),
        )?;
        match args[1] {
            api::Value::Boolean(cancel) => Ok(cancel),
            _ => Err("invalid BeforeSave Cancel writeback (dc-hsa5.12)".into()),
        }
    }

    pub fn before_save_model(&self, save_as_ui: bool, max_steps: u64) -> Result<bool, String> {
        let mut args = [api::Value::Boolean(save_as_ui), api::Value::Boolean(false)];
        self.raise("BeforeSave", &mut args, None, max_steps, next_lease())?;
        match args[1] {
            api::Value::Boolean(cancel) => Ok(cancel),
            _ => Err("invalid BeforeSave Cancel writeback (dc-hsa5.12)".into()),
        }
    }

    fn raise(
        &self,
        name: &str,
        arguments: &mut [api::Value],
        context: Option<&dyn EventReadContext>,
        max_steps: u64,
        lease: u64,
    ) -> Result<(), String> {
        if callback_active() {
            return api::with_native_callback_context(|budget| {
                self.raise_in_context(name, arguments, context, budget, lease, true)
            })
            .map_err(|e| format!("nested event context (dc-hsa5.12): {e}"))?;
        }
        let mut budget = api::TypedContext {
            remaining_blocks: max_steps,
        };
        self.raise_in_context(name, arguments, context, &mut budget, lease, false)
    }

    fn raise_in_context(
        &self,
        name: &str,
        arguments: &mut [api::Value],
        context: Option<&dyn EventReadContext>,
        budget: &mut api::TypedContext,
        lease: u64,
        nested: bool,
    ) -> Result<(), String> {
        let ids = self.hub.matching_ids(name);
        let refused = RefCell::new(None);
        let result = crate::runtime::spreadsheet::with_context(&EventSpreadsheet, || {
            EVENT_SCOPE.set((context, lease, &refused), || {
                api::with_native_call(budget, || {
                    for id in ids {
                        // Release the source table before entering user code. A reset/drop can
                        // unsubscribe a later callback; a newly subscribed callback waits for a new event.
                        if let Some(callback) = self.hub.callback(id) {
                            callback(arguments).map_err(|number| {
                                crate::runtime::Error::Backend(format!(
                                    "VBA {name} event failed ({number}, dc-hsa5.12)"
                                ))
                            })?;
                        }
                    }
                    Ok(())
                })
            })
        })
        .map_err(|e| format!("{e} (dc-hsa5.12)"));
        // Nested handlers leave the fatal fault on the shared context so the
        // triggering VBA call cannot catch it and continue. Only the outer event
        // boundary drains it before a later top-level event receives a new budget.
        let fault = api::take_unhandled_event_fault(budget);
        let fault_reason = fault.as_ref().map(|fault| {
            format!(
                "VBA {name} event failed ({}: {}{}, dc-hsa5.12)",
                fault.number,
                fault.description,
                fault
                    .source
                    .as_ref()
                    .map(|source| format!(" [{source}]"))
                    .unwrap_or_default(),
            )
        });
        if nested && let Some(fault) = fault {
            api::publish_unhandled_event_fault(budget, fault);
        }
        match (refused.into_inner(), fault_reason) {
            (Some(reason), _) => Err(format!("{reason} (dc-hsa5.12)")),
            (None, Some(reason)) => Err(reason),
            (None, None) => result,
        }
    }
}

/// One argument of a procedure a host runs by name.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcedureArg {
    Empty,
    Bool(bool),
    Number(f64),
    Text(String),
}

/// What a procedure a host ran by name returned.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcedureResult {
    /// A Sub, or a Function that returned Empty.
    Empty,
    Bool(bool),
    Number(f64),
    Text(String),
    /// A value of another kind (an error value, an object, an array): its VBA type name.
    Other(String),
}

/// Why a procedure run by name did not return.
#[derive(Clone, Debug, PartialEq)]
pub enum ProcedureError {
    /// No public Sub or Function of that module and name (or the name is ambiguous).
    Unknown(String),
    /// More or fewer arguments than the procedure takes.
    Arguments,
    /// A VBA run-time error that reached the caller (after any `On Error` handling).
    Runtime { number: i32, description: String, source: String },
    /// A host member the run needed is not modelled; the owning work package follows.
    Refused(String),
    Budget,
    Backend(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcedureKind {
    Sub,
    Function,
}

/// One declared parameter of a catalogued procedure.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcedureParameter {
    pub name: String,
    pub optional: bool,
    pub by_ref: bool,
    pub param_array: bool,
}

/// One Sub or Function of the project, as its catalogue lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcedureEntry {
    pub module: String,
    pub name: String,
    pub kind: ProcedureKind,
    pub public: bool,
    pub parameters: Vec<ProcedureParameter>,
}

impl super::LoadedProject {
    /// Every Sub and Function the project declares (properties are not entry points).
    pub fn procedures(&self) -> Vec<ProcedureEntry> {
        api::Session::procedures(&*self.session)
            .iter()
            .filter_map(|p| {
                let kind = match p.kind {
                    api::ProcedureKind::Sub => ProcedureKind::Sub,
                    api::ProcedureKind::Function => ProcedureKind::Function,
                    _ => return None,
                };
                Some(ProcedureEntry {
                    module: p.module.clone(),
                    name: p.name.clone(),
                    kind,
                    public: p.visibility == api::Visibility::Public,
                    parameters: p
                        .parameters
                        .iter()
                        .map(|q| ProcedureParameter {
                            name: q.name.clone(),
                            optional: q.optional,
                            by_ref: q.by_ref,
                            param_array: q.param_array,
                        })
                        .collect(),
                })
            })
            .collect()
    }

    /// Run the public Sub or Function `module.name` once, in a command context: the host's
    /// workbook model is live (`Range`, `Application`, `ThisWorkbook`), as it is for a document
    /// event. Not an event: no handler is raised by the call itself.
    pub fn run_procedure(
        &self,
        module: &str,
        name: &str,
        arguments: &[ProcedureArg],
        max_steps: u64,
    ) -> Result<ProcedureResult, ProcedureError> {
        self.run_procedure_recorded(module, name, arguments, max_steps).0
    }

    /// [`run_procedure`](Self::run_procedure) with the object-model members the policy denied
    /// during the call, as `(capability, descriptor member)`, caught by VBA or not.
    pub fn run_procedure_recorded(
        &self,
        module: &str,
        name: &str,
        arguments: &[ProcedureArg],
        max_steps: u64,
    ) -> (Result<ProcedureResult, ProcedureError>, Vec<(String, String)>) {
        // A nested run (`Application.Run` from a procedure) leaves its denials in the log for the
        // outermost run to return.
        let depth = RUN_DEPTH.with(|d| d.replace(d.get() + 1));
        let start = if depth == 0 {
            OBJECT_DENIALS.with(|d| d.borrow_mut().clear());
            0
        } else {
            OBJECT_DENIALS.with(|d| d.borrow().len())
        };
        let result = self.run_inner(module, name, arguments, max_steps);
        RUN_DEPTH.with(|d| d.set(depth));
        let denials = OBJECT_DENIALS.with(|d| {
            if depth == 0 {
                std::mem::take(&mut *d.borrow_mut())
            } else {
                d.borrow()[start..].to_vec()
            }
        });
        (result, denials)
    }

    fn run_inner(
        &self,
        module: &str,
        name: &str,
        arguments: &[ProcedureArg],
        max_steps: u64,
    ) -> Result<ProcedureResult, ProcedureError> {
        use calcvba_runtime::Variant;
        let kind = self
            .procedures()
            .into_iter()
            .find(|p| {
                p.public
                    && p.module.eq_ignore_ascii_case(module)
                    && p.name.eq_ignore_ascii_case(name)
            })
            .map(|p| p.kind)
            .ok_or_else(|| ProcedureError::Unknown(format!("{module}.{name}")))?;
        let bound = api::Session::bind(
            &*self.session,
            module,
            name,
            match kind {
                ProcedureKind::Sub => api::ProcedureKind::Sub,
                ProcedureKind::Function => api::ProcedureKind::Function,
            },
        )
        .map_err(map_error)?;
        let args: Vec<Variant> = arguments
            .iter()
            .map(|a| match a {
                ProcedureArg::Empty => Variant::empty(),
                ProcedureArg::Bool(b) => Variant::from_bool(*b),
                ProcedureArg::Number(n) => Variant::from_f64(*n),
                ProcedureArg::Text(t) => Variant::from_string(t.as_str()),
            })
            .collect();
        let refused = RefCell::new(None);
        let lease = next_lease();
        let failure: RefCell<Option<api::Error>> = RefCell::new(None);
        let outcome = crate::runtime::spreadsheet::with_context(&EventSpreadsheet, || {
            EVENT_SCOPE.set((None, lease, &refused), || {
                api::Session::call_bound(&*self.session, &bound, &args, max_steps).map_err(|e| {
                    *failure.borrow_mut() = Some(e);
                    crate::runtime::Error::Backend(String::new())
                })
            })
        });
        if let Some(reason) = refused.into_inner() {
            return Err(ProcedureError::Refused(reason));
        }
        let value = match outcome {
            Ok(value) => value,
            Err(other) => {
                return Err(match failure.into_inner() {
                    Some(error) => map_error(error),
                    None => ProcedureError::Backend(other.to_string()),
                });
            }
        };
        Ok(match value.vtype() {
            calcvba_runtime::VarType::Empty => ProcedureResult::Empty,
            calcvba_runtime::VarType::Boolean => {
                ProcedureResult::Bool(value.as_bool().unwrap_or(false))
            }
            calcvba_runtime::VarType::String => {
                ProcedureResult::Text(value.as_bstr().map(|s| s.to_string()).unwrap_or_default())
            }
            other => match value.as_f64() {
                Some(n) if !matches!(other, calcvba_runtime::VarType::Error) => {
                    ProcedureResult::Number(n)
                }
                _ => ProcedureResult::Other(format!("{other:?}")),
            },
        })
    }
}

fn map_error(error: api::Error) -> ProcedureError {
    match error {
        api::Error::UnknownProcedure(name) => ProcedureError::Unknown(name),
        api::Error::ArgumentMismatch => ProcedureError::Arguments,
        api::Error::BudgetExceeded => ProcedureError::Budget,
        api::Error::Runtime { number, description, vba_source } => {
            ProcedureError::Runtime { number, description, source: vba_source }
        }
        other => ProcedureError::Backend(other.to_string()),
    }
}

fn next_lease() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("event read lease exhausted")
}
